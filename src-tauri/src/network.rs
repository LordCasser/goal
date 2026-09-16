//! Provider connection policy shared by persistence, probes and sampling.
//! This module never changes process environment or operating-system settings.

use std::net::IpAddr;

use reqwest::{ClientBuilder, Proxy, Url};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConnectionSettings {
    #[default]
    Auto,
    Direct,
    Proxy {
        url: String,
    },
}

// Even malformed, not-yet-validated input must never print URL credentials.
impl std::fmt::Debug for ConnectionSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("Auto"),
            Self::Direct => f.write_str("Direct"),
            Self::Proxy { .. } => f.write_str("Proxy { url: [REDACTED] }"),
        }
    }
}

fn invalid_proxy() -> AppError {
    AppError::validation("invalid_proxy_url", "Enter an HTTP, HTTPS, SOCKS5 or SOCKS5h proxy URL with a host and explicit port (1–65535), without credentials, path, query or fragment.")
}

fn proxy_url(raw: &str) -> AppResult<Url> {
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) || raw.contains('\\') {
        return Err(invalid_proxy());
    }
    let url = Url::parse(raw).map_err(|_| invalid_proxy())?;
    let authority = raw
        .split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(""))
        .ok_or_else(invalid_proxy)?;
    // URL parsers discard explicit default ports (80/443), so inspect the
    // authority as well. Parsing above still validates IPv6 and host syntax.
    let port = authority
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok());
    if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h")
        || url.host_str().is_none_or(str::is_empty)
        || port.is_none_or(|port| port == 0)
        || authority.contains('@')
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid_proxy());
    }
    Ok(url)
}

impl ConnectionSettings {
    pub fn validate(&self) -> AppResult<()> {
        if let Self::Proxy { url } = self {
            proxy_url(url)?;
        }
        Ok(())
    }

    /// Decide from the final destination, never from the proxy's host. A
    /// localhost proxy may still carry remote model traffic normally.
    pub(crate) fn apply(
        &self,
        builder: ClientBuilder,
        destination: &str,
    ) -> AppResult<ClientBuilder> {
        self.validate()?;
        if is_loopback_url(destination) {
            return Ok(builder.no_proxy());
        }
        match self {
            Self::Auto => Ok(builder),
            Self::Direct => Ok(builder.no_proxy()),
            Self::Proxy { url } => Ok(builder
                .no_proxy()
                .proxy(Proxy::all(proxy_url(url)?).map_err(|_| invalid_proxy())?)),
        }
    }
}

fn is_loopback_url(raw_url: &str) -> bool {
    let Ok(url) = Url::parse(raw_url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.');
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => address.is_loopback(),
        Ok(IpAddr::V6(address)) => {
            address.is_loopback()
                || address
                    .to_ipv4_mapped()
                    .is_some_and(|mapped| mapped.is_loopback())
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    const ROUTE_SUBPROCESS: &str = "GOAL_NETWORK_ROUTE_SUBPROCESS";

    async fn read_http_head(stream: &mut TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        let _ = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let read = stream.read(&mut chunk).await?;
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Ok::<(), std::io::Error>(())
        })
        .await;
        request
    }

    /// A one-request HTTP endpoint. The request is retained so proxy tests can
    /// distinguish an absolute-form proxy request from a direct origin form.
    async fn spawn_http_endpoint(
        response: &'static [u8],
    ) -> (std::net::SocketAddr, Arc<AtomicBool>, Arc<Mutex<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let saw_request = Arc::new(AtomicBool::new(false));
        let request = Arc::new(Mutex::new(Vec::new()));
        let saw_request_for_task = saw_request.clone();
        let request_for_task = request.clone();
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let bytes = read_http_head(&mut stream).await;
            *request_for_task.lock().unwrap() = bytes;
            saw_request_for_task.store(true, Ordering::Release);
            let _ = stream.write_all(response).await;
        });
        (address, saw_request, request)
    }

    fn one_second_client() -> reqwest::ClientBuilder {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_secs(1))
    }

    async fn wait_for_request(flag: &AtomicBool) {
        for _ in 0..100 {
            if flag.load(Ordering::Acquire) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    #[test]
    fn connection_shape_round_trips_without_stale_proxy_fields() {
        for value in [
            serde_json::json!({"mode":"auto"}),
            serde_json::json!({"mode":"direct"}),
            serde_json::json!({"mode":"proxy","url":"http://localhost:7890"}),
        ] {
            let config: ConnectionSettings = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(config).unwrap(), value);
        }
        let direct_with_stale_url: ConnectionSettings = serde_json::from_value(
            serde_json::json!({"mode":"direct","url":"http://localhost:7890"}),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(direct_with_stale_url).unwrap(),
            serde_json::json!({"mode":"direct"})
        );
        assert!(
            serde_json::from_value::<ConnectionSettings>(serde_json::json!({"mode":"proxy"}))
                .is_err()
        );
    }

    #[test]
    fn validates_proxy_addresses_without_echoing_credentials() {
        for url in [
            "http://localhost:7890",
            "https://proxy.example:443/",
            "http://proxy:80",
            "socks5://127.0.0.1:1080",
            "socks5h://[::1]:1080/",
        ] {
            ConnectionSettings::Proxy { url: url.into() }
                .validate()
                .unwrap();
        }
        for url in [
            "",
            "http://proxy",
            "http://proxy:0",
            "http://proxy:65536",
            "http://:7890",
            "ftp://proxy:21",
            "socks4://proxy:1080",
            "http://user:secret@proxy:7890",
            "http://@proxy:7890",
            "http://proxy:7890/path",
            "http://proxy:7890/?token=secret",
            "http://proxy:7890/#secret",
            " http://proxy:7890",
            "http://pro\nxy:7890",
            "http://proxy:7890\\path",
            "http://[::1]",
        ] {
            let config = ConnectionSettings::Proxy { url: url.into() };
            let error = config.validate().unwrap_err();
            assert!(
                matches!(&error, AppError::Validation { code, .. } if code == "invalid_proxy_url"),
                "{url}"
            );
            assert!(!error.to_string().contains("secret"));
            assert!(!format!("{config:?}").contains("secret"));
        }
    }
    #[test]
    fn loopback_url_detection_covers_localhost_and_loopback_addresses() {
        for url in [
            "http://localhost/v1/chat/completions",
            "http://LOCALHOST./v1/chat/completions",
            "http://127.0.0.1:11434/v1/chat/completions",
            "http://127.42.8.9/v1/chat/completions",
            "http://[::1]:11434/v1/chat/completions",
            "http://[::ffff:127.0.0.1]:11434/v1/chat/completions",
        ] {
            assert!(is_loopback_url(url), "expected loopback URL: {url}");
        }
    }

    #[test]
    fn loopback_url_detection_rejects_nonlocal_hosts() {
        for url in [
            "https://api.example.com/v1/chat/completions",
            "http://127.0.0.1.example.com/v1/chat/completions",
            "http://[::2]:11434/v1/chat/completions",
            "not a URL",
        ] {
            assert!(!is_loopback_url(url), "expected remote URL: {url}");
        }
    }

    #[tokio::test]
    async fn direct_resolves_non_loopback_domain_to_local_mock() {
        let (address, saw_request, _) = spawn_http_endpoint(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        )
        .await;
        let destination = format!("http://llm.example.test:{}/v1", address.port());
        let client = ConnectionSettings::Direct
            .apply(
                one_second_client().resolve("llm.example.test", address),
                &destination,
            )
            .unwrap()
            .build()
            .unwrap();

        let response = client.get(&destination).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        wait_for_request(&saw_request).await;
        assert!(saw_request.load(Ordering::Acquire));
    }

    /// Proxy environment variables are process-global. Keep all environment
    /// mutation inside this ignored child so the suite can run in parallel.
    #[tokio::test]
    #[ignore = "spawned as a subprocess by explicit_proxy_ignores_system_proxy_and_no_proxy"]
    async fn explicit_proxy_subprocess_helper() {
        if std::env::var(ROUTE_SUBPROCESS).is_err() {
            return;
        }

        // This host is not in NO_PROXY, and every inherited proxy is broken.
        // Direct must still reach the mock origin through the custom resolver.
        let (direct_address, direct_hit, _) = spawn_http_endpoint(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        )
        .await;
        let direct_destination = format!("http://direct.example.test:{}/v1", direct_address.port());
        let direct = ConnectionSettings::Direct
            .apply(
                one_second_client().resolve("direct.example.test", direct_address),
                &direct_destination,
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            direct
                .get(&direct_destination)
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::OK
        );
        assert!(direct_hit.load(Ordering::Acquire));

        let (target_address, target_hit, _) = spawn_http_endpoint(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        )
        .await;
        let destination = format!("http://llm.example.test:{}/v1", target_address.port());

        let (proxy_address, proxy_hit, proxy_request) = spawn_http_endpoint(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        )
        .await;
        let proxy = ConnectionSettings::Proxy {
            url: format!("http://127.0.0.1:{}", proxy_address.port()),
        };
        let client = proxy
            .apply(
                one_second_client().resolve("llm.example.test", target_address),
                &destination,
            )
            .unwrap()
            .build()
            .unwrap();
        let response = client.get(&destination).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        wait_for_request(&proxy_hit).await;
        assert!(proxy_hit.load(Ordering::Acquire));
        let proxy_request = String::from_utf8(proxy_request.lock().unwrap().clone()).unwrap();
        assert!(proxy_request.starts_with(&format!("GET {destination} HTTP/1.1")));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!target_hit.load(Ordering::Acquire));

        // A failed explicit proxy must not fall through to a directly
        // reachable mock origin.
        let (bad_target_address, bad_target_hit, _) = spawn_http_endpoint(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        )
        .await;
        let bad_destination = format!("http://llm.example.test:{}/v1", bad_target_address.port());
        let dropped = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dropped_port = dropped.local_addr().unwrap().port();
        drop(dropped);
        let bad_proxy = ConnectionSettings::Proxy {
            url: format!("http://127.0.0.1:{dropped_port}"),
        };
        let client = bad_proxy
            .apply(
                one_second_client().resolve("llm.example.test", bad_target_address),
                &bad_destination,
            )
            .unwrap()
            .build()
            .unwrap();
        let result =
            tokio::time::timeout(Duration::from_secs(1), client.get(&bad_destination).send())
                .await
                .unwrap();
        assert!(result.is_err());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!bad_target_hit.load(Ordering::Acquire));

        // HTTPS proxy setup reaches the configured listener and starts TLS;
        // certificate verification remains enabled, so this intentionally
        // fails after the ClientHello is observed.
        let tls_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tls_address = tls_listener.local_addr().unwrap();
        let tls_bytes = Arc::new(Mutex::new(Vec::new()));
        let tls_bytes_for_task = tls_bytes.clone();
        tokio::spawn(async move {
            let Ok((mut stream, _)) = tls_listener.accept().await else {
                return;
            };
            let mut bytes = [0_u8; 8];
            let read = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut bytes))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or(0);
            *tls_bytes_for_task.lock().unwrap() = bytes[..read].to_vec();
        });
        let tls_proxy = ConnectionSettings::Proxy {
            url: format!("https://127.0.0.1:{}", tls_address.port()),
        };
        let tls_client = tls_proxy
            .apply(one_second_client(), &destination)
            .unwrap()
            .build()
            .unwrap();
        let result =
            tokio::time::timeout(Duration::from_secs(1), tls_client.get(&destination).send())
                .await
                .unwrap();
        assert!(result.is_err());
        tokio::time::sleep(Duration::from_millis(50)).await;
        let tls_bytes = tls_bytes.lock().unwrap().clone();
        assert!(tls_bytes.len() >= 3);
        assert_eq!(tls_bytes[0], 0x16, "expected TLS handshake record");
        assert_eq!(tls_bytes[1], 0x03, "expected TLS record version");
    }

    #[tokio::test]
    async fn explicit_proxy_ignores_system_proxy_and_no_proxy() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "network::tests::explicit_proxy_subprocess_helper",
                "--ignored",
                "--quiet",
            ])
            .env(ROUTE_SUBPROCESS, "1")
            .env("HTTP_PROXY", "http://127.0.0.1:9")
            .env("http_proxy", "http://127.0.0.1:9")
            .env("HTTPS_PROXY", "http://127.0.0.1:9")
            .env("https_proxy", "http://127.0.0.1:9")
            .env("ALL_PROXY", "http://127.0.0.1:9")
            .env("all_proxy", "http://127.0.0.1:9")
            .env("NO_PROXY", "llm.example.test")
            .env("no_proxy", "llm.example.test")
            .env_remove("REQUEST_METHOD")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "explicit proxy subprocess failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    async fn spawn_socks5_endpoint() -> (std::net::SocketAddr, Arc<Mutex<Option<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let address_type = Arc::new(Mutex::new(None));
        let address_type_for_task = address_type.clone();
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut greeting = [0_u8; 3];
            if tokio::time::timeout(Duration::from_secs(1), stream.read_exact(&mut greeting))
                .await
                .is_err()
                || greeting != [0x05, 0x01, 0x00]
            {
                return;
            }
            if stream.write_all(&[0x05, 0x00]).await.is_err() {
                return;
            }
            let mut header = [0_u8; 4];
            if tokio::time::timeout(Duration::from_secs(1), stream.read_exact(&mut header))
                .await
                .is_err()
            {
                return;
            }
            if header[0] != 0x05 || header[1] != 0x01 || header[2] != 0x00 {
                return;
            }
            *address_type_for_task.lock().unwrap() = Some(header[3]);
            let address_len = match header[3] {
                0x01 => 4,
                0x03 => {
                    let mut length = [0_u8; 1];
                    if stream.read_exact(&mut length).await.is_err() {
                        return;
                    }
                    length[0] as usize
                }
                0x04 => 16,
                _ => return,
            };
            let mut address_and_port = vec![0_u8; address_len + 2];
            if stream.read_exact(&mut address_and_port).await.is_err() {
                return;
            }
            let bound = [0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0, 80];
            if stream.write_all(&bound).await.is_err() {
                return;
            }
            let request = read_http_head(&mut stream).await;
            if !request.starts_with(b"GET /v1 HTTP/1.1\r\n") {
                return;
            }
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await;
        });
        (address, address_type)
    }

    #[tokio::test]
    async fn socks5_and_socks5h_send_ip_and_domain_address_types() {
        let (socks5_address, socks5_type) = spawn_socks5_endpoint().await;
        let destination = "http://llm.example.test:18080/v1";
        let socks5 = ConnectionSettings::Proxy {
            url: format!("socks5://127.0.0.1:{}", socks5_address.port()),
        };
        let client = socks5
            .apply(
                one_second_client().resolve("llm.example.test", "192.0.2.1:18080".parse().unwrap()),
                destination,
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            client.get(destination).send().await.unwrap().status(),
            reqwest::StatusCode::OK
        );
        assert_eq!(*socks5_type.lock().unwrap(), Some(0x01));

        let (socks5h_address, socks5h_type) = spawn_socks5_endpoint().await;
        let destination = "http://llm.example.test:18080/v1";
        let socks5h = ConnectionSettings::Proxy {
            url: format!("socks5h://127.0.0.1:{}", socks5h_address.port()),
        };
        let client = socks5h
            .apply(one_second_client(), destination)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            client.get(destination).send().await.unwrap().status(),
            reqwest::StatusCode::OK
        );
        assert_eq!(*socks5h_type.lock().unwrap(), Some(0x03));
    }
}

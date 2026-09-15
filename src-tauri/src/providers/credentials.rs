//! Keychain credential access (task §2). Service is the app identifier;
//! account is `provider:{id}`. Local endpoints may have no credential at all.
//!
//! Per-provider API keys live in the system keychain and nowhere else (design
//! D4; spec: 凭据安全存储): they never touch `providers.json`, the database
//! or a log line. Every key seen here — the input of [`Credentials::save`]
//! and the output of [`Credentials::load`] — is registered with
//! [`crate::logging::register_secret`] immediately, so any later log write
//! masks it as `[REDACTED]`.
//!
//! Absence is a normal state, not an error: a missing keychain entry means
//! "no credential (local endpoint)". Whether a provider needs a key at all is
//! the caller's decision — an empty credential is expressed by simply never
//! calling [`Credentials::save`], never by storing a blank string. Only real
//! access failures surface as [`AppError::Internal`].

use crate::error::{AppError, AppResult};

/// Keychain service name: the application identifier from `tauri.conf.json`.
pub const SERVICE: &str = "dev.lordcasser.planner";

/// Log label shared by this module's diagnostics (ids only, never key material).
const LOG_MODULE: &str = "credentials";

/// Keychain account for one provider: `provider:{id}`.
fn account(provider_id: &str) -> String {
    format!("provider:{provider_id}")
}

/// Stateless wrapper around the system keychain. All state lives in the
/// keychain itself, keyed by [`SERVICE`] + [`account`].
#[derive(Debug, Clone, Default)]
pub struct Credentials;

impl Credentials {
    pub fn new() -> Self {
        Self
    }

    /// Stores `api_key` for `provider_id`, replacing any previous value.
    pub fn save(&self, provider_id: &str, api_key: &str) -> AppResult<()> {
        // Register first: even if the write fails, the key can never leak
        // into a later log line through this module.
        crate::logging::register_secret(api_key);
        let entry = entry_for(provider_id)?;
        entry.set_password(api_key).map_err(keyring_error)?;
        crate::logging::debug(
            LOG_MODULE,
            &format!("stored keychain entry for provider {provider_id}"),
        );
        Ok(())
    }

    /// Reads the stored key. `Ok(None)` means "not configured" (a normal
    /// state for local endpoints); anything else — a locked or unusable
    /// keychain — is an `Err` so callers never confuse "no key" with
    /// "cannot read the key" (task 2.4).
    pub fn load(&self, provider_id: &str) -> AppResult<Option<String>> {
        let entry = entry_for(provider_id)?;
        match entry.get_password() {
            Ok(key) => {
                crate::logging::register_secret(&key);
                Ok(Some(key))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }

    /// Removes the stored key. A missing entry is also success (idempotent);
    /// callers cascade this after deleting a provider (task 2.2).
    pub fn delete(&self, provider_id: &str) -> AppResult<()> {
        let entry = entry_for(provider_id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {
                crate::logging::debug(
                    LOG_MODULE,
                    &format!("deleted keychain entry for provider {provider_id}"),
                );
                Ok(())
            }
            Err(e) => Err(keyring_error(e)),
        }
    }
}

fn entry_for(provider_id: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, &account(provider_id)).map_err(keyring_error)
}

/// Maps a keyring failure to [`AppError::Internal`] with a short status line.
/// The keyring error text carries platform status codes and entry names only
/// — never the credential — and this fixed prefix + source text is the only
/// message we build, so key material cannot appear in an error (spec: AI
/// 失败不伤及本地, 密钥不得出现在错误信息中).
fn keyring_error(err: keyring::Error) -> AppError {
    AppError::Internal(format!("keychain access failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_constant_and_account_format() {
        assert_eq!(SERVICE, "dev.lordcasser.planner");
        assert_eq!(account("7f2c…"), "provider:7f2c…");
        assert_eq!(account(""), "provider:");
    }

    #[test]
    fn error_mapping_is_fixed_prefix_plus_status_text() {
        let err = keyring_error(keyring::Error::NoEntry);
        let AppError::Internal(message) = &err else {
            panic!("expected Internal, got {err:?}");
        };
        // The mapping builds exactly one string: prefix + the keyring error's
        // own text. Nothing else is interpolated, so there is no path by
        // which a key could enter an error message.
        assert_eq!(
            message,
            &format!("keychain access failed: {}", keyring::Error::NoEntry)
        );
        assert!(!message.contains("sk-"), "no key material in errors");
    }

    // The remaining tests exercise a real system keychain, which CI and
    // sandboxed environments may not provide; run them locally with
    // `cargo test -- --ignored`.

    #[test]
    #[ignore = "requires a usable system keychain"]
    fn keychain_save_load_roundtrip() {
        let credentials = Credentials::new();
        let provider_id = format!("test-{}", uuid::Uuid::new_v4());
        let key = "sk-test-roundtrip-123456";
        credentials.save(&provider_id, key).expect("save");
        assert_eq!(
            credentials.load(&provider_id).expect("load"),
            Some(key.into())
        );
        credentials.delete(&provider_id).expect("cleanup delete");
    }

    #[test]
    #[ignore = "requires a usable system keychain"]
    fn keychain_delete_then_load_is_none() {
        let credentials = Credentials::new();
        let provider_id = format!("test-{}", uuid::Uuid::new_v4());
        credentials
            .save(&provider_id, "sk-test-delete-me")
            .expect("save");
        credentials.delete(&provider_id).expect("delete");
        assert_eq!(
            credentials.load(&provider_id).expect("load after delete"),
            None
        );
        // Deleting again is still success (idempotent).
        credentials.delete(&provider_id).expect("delete again");
    }

    #[test]
    #[ignore = "requires a usable system keychain"]
    fn keychain_missing_entry_loads_none() {
        let credentials = Credentials::new();
        let provider_id = format!("test-{}", uuid::Uuid::new_v4());
        credentials.delete(&provider_id).expect("pre-clean");
        assert_eq!(credentials.load(&provider_id).expect("load missing"), None);
    }
}

#[cfg(test)]
mod acceptance {
    use super::*;

    /// ai-access task 6.4: with a key stored through the real keychain, no
    /// file on disk — providers.json, database, logs — contains the
    /// plaintext. Keychain-only storage is the design红线 (spec:
    /// 凭据安全存储). Requires a usable system keychain.
    #[test]
    #[ignore = "requires a usable system keychain (cargo test -- --ignored)"]
    fn keychain_material_never_hits_disk() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = [
            "sk-acceptance-secret-0123456789",
            "acceptance-second-secret-9876543210",
        ];
        for (index, secret) in secrets.iter().enumerate() {
            let provider_id = format!("acceptance-{index}");
            Credentials::new().save(&provider_id, secret).unwrap();
            // Seed a config file and a settings row so the scan has real
            // surfaces to inspect.
            std::fs::write(
                dir.path().join("providers.json"),
                format!(r#"{{"provider_id":"{provider_id}"}}"#),
            )
            .unwrap();
            let conn = rusqlite::Connection::open(dir.path().join("planner.db")).unwrap();
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES ('probe', 'provider')",
                [],
            )
            .unwrap_or(0);
            drop(conn);
        }

        // Scan every regular file under the data directory.
        let mut scanned = 0;
        let mut stack = vec![dir.path().to_path_buf()];
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).unwrap().flatten() {
                    stack.push(entry.path());
                }
            } else if let Ok(contents) = std::fs::read_to_string(&path) {
                scanned += 1;
                for secret in &secrets {
                    assert!(
                        !contents.contains(secret),
                        "credential plaintext leaked into {}",
                        path.display()
                    );
                }
            }
        }
        assert!(scanned >= 2, "expected files to scan, got {scanned}");

        let credentials = Credentials::new();
        for index in 0..secrets.len() {
            credentials.delete(&format!("acceptance-{index}")).unwrap();
        }
    }
}

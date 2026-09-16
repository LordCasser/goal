# Localhost provider connectivity audit — 2026-09-16

## Confirmed defect

`sampling::sample` created a reqwest client with default system proxy discovery for every URL. A desktop process can inherit HTTP/HTTPS/ALL proxy settings without inheriting the terminal's NO_PROXY. This allows a terminal curl request to reach a loopback model service while Goal sends the same destination through an unavailable proxy.

The fix is in the shared sampling HTTP boundary, after adapters assemble the final URL. `localhost` (case insensitive, optional trailing dot), IPv4 loopback, IPv6 `::1`, and IPv4-mapped IPv6 loopback explicitly bypass proxies. Other hosts retain the existing proxy behavior. No process environment, system configuration, provider schema, credentials, or protocol payload changes are required. Redirects remain disabled.

## Validation

- URL classification covers the loopback forms above and rejects lookalike/remote hosts.
- An isolated test process has every proxy variable set to a closed local port and no NO_PROXY. The old default reqwest client fails against a local mock; the fixed sampling client receives and decodes the mock's SSE response.
- A separate isolated process sends a remote-address request through a local mock HTTP proxy and receives SSE, proving remote proxy behavior is retained. No external provider or real credentials are used.
- `cargo test --manifest-path src-tauri/Cargo.toml sampling:: -- --nocapture`: 51 passed, 0 failed, 3 ignored. Two ignored helpers are explicitly executed by their parent tests; the remaining ignored test requires a real provider.

## Limits of the finding

This confirms a transport defect, not the exact cause of an unidentified user's failure. Goal tests each configured model using a streaming POST to its selected protocol endpoint. A curl GET of `/v1/models`, or a non-streaming generation request, is not equivalent. Base URL must exclude the operation suffix; model IDs, API format, custom headers, and SSE compatibility still need to match the local service. No IPv4/IPv6 fallback defect was demonstrated in this audit.

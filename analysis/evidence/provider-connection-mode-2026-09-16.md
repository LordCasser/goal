# Provider connection modes — 2026-09-16

Scope: the requested first two steps only: explicit loopback bypass and per-provider Auto / Direct / Specified proxy. Windows native ProxyOverride, PAC/WPAD and proxy authentication remain outside this change.

## Architecture and behavior

- `network::ConnectionSettings` is the shared configuration/request contract. `network.rs` validates proxy addresses, classifies the final destination and applies the reqwest policy.
- localhost (case-insensitive, optional terminal dot), IPv4 loopback, IPv6 loopback and IPv4-mapped loopback always connect directly.
- Remote Auto retains reqwest defaults. Direct disables proxies. Specified proxy disables automatic proxy selection, installs only the chosen proxy, and never falls back to a direct request.
- Connection probes, all configured models, ordinary structured AI generation and agent requests carry the same settings through the common sampling client.
- Advanced settings use the existing Select and form layout. Only proxy mode displays a URL. Invalid input blocks saving; changing connection settings disables the saved-configuration retest. Test-and-save uses the draft, preserves the old configuration on failure and commits only after all models pass.
- Proxy URLs require HTTP/HTTPS/SOCKS5/SOCKS5h, host and explicit port 1–65535. Credentials, query, fragment, non-root path, whitespace and backslashes are rejected. Debug and validation errors do not echo the URL. No keychain fields or system settings were added.

## Automated verification

- `npm test`: 62 files, **467 passed**.
- `npm run build`: TypeScript and Vite passed. Existing large-chunk warning remains.
- `cargo test --manifest-path src-tauri/Cargo.toml`: **547 passed, 0 failed, 11 ignored** across 17 suites. Ignored entries include explicitly invoked subprocess helpers, real-provider opt-in tests and a documentation example.
- `openspec validate --all --strict`: **31 passed, 0 failed**.
- `git diff --check`: passed.
- Tauri macOS debug app bundle: passed, then signed and verified with the existing pinned Goal identity.

Focused network checks use local mock listeners and isolated subprocess environments:

- Auto keeps the configured environment proxy for a remote hostname; loopback bypasses an unreachable environment proxy.
- Direct reaches a non-loopback hostname resolved to a mock origin even when every proxy environment variable points at a broken proxy and the hostname is absent from NO_PROXY.
- Explicit HTTP proxy receives an absolute-form request even when system proxy variables and NO_PROXY disagree with it. A failed explicit proxy never reaches the separately reachable mock origin.
- All three API protocols use explicit proxy routing and bypass proxies for loopback under every mode.
- HTTPS proxy test verifies connection to the selected listener and a TLS ClientHello, then intentionally fails. It does not claim a complete authenticated TLS proxy transaction and does not disable certificate verification.
- SOCKS5 and SOCKS5h complete a mock handshake and HTTP exchange; the tests distinguish IP versus domain address types.
- Persistence, missing-field Auto defaults, malformed URLs, redacted errors, draft probe/save, saved retest and both AI request constructors are covered.

## UI and review

- Disposable browser fixture used the real ProviderForm and i18n with in-memory IPC responses. Verified Chinese and English, advanced expansion, empty proxy without premature red error, URL validation, failure preserving the draft and saved value, successful proxy save, and Direct save omitting the old proxy URL.
- Real macOS Goal.app: opened Settings → AI models → Add provider → Advanced connection settings; checked the mode menu, conditional proxy input, scrolling and reachable footer. Closed the unsaved form; no real provider configuration was changed.
- Read-only implementation review found no actionable defects in routing, serialization, save/probe state or credential redaction.

The enterprise Windows registry/ProxyOverride environment and a physical Linux desktop were not available for this verification. Tests validate the portable policy and reqwest behavior on the local macOS host; they do not substitute for platform-specific acceptance testing.

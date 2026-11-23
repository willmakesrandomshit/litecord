# Discord Zero — Security & Account Safety

## Core Security Principles

1. **No Token Scraping or Client Spoofing:** Discord Zero will never extract tokens from the official Discord client, bypass authentication checks, or impersonate official client integrity signals.
2. **Supported Interfaces Only:** Any live Discord connectivity (Phase C+) must use documented, supported authentication and API boundaries.
3. **Credential Protection:** Sensitive credentials will be stored using OS-native credential stores (e.g., Windows Credential Manager, macOS Keychain, Linux Secret Service) rather than plaintext config files.
4. **Plugin Isolation:** Third-party plugins (Phase D+) will run inside a sandboxed JavaScript runtime with explicit capability permissions and no unrestricted filesystem or credential access.
5. **Memory Safety:** `#![forbid(unsafe_code)]` is enforced on `zero-core` and `zero-storage`.

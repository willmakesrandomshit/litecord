# Discord Zero — Vencord Compatibility Matrix & Architecture

**Status:** Phase D Implemented (`zero-plugins` + `zero-vencord` + `zero-themes`)

Discord Zero is a native Rust desktop application (`iced`), not an Electron/Chromium browser. It does not bundle Discord's Webpack module graph, React DOM, or `FluxDispatcher`.

## 1. Compatibility Classification

| Category | Description | Representative Examples | Status in Discord Zero |
|---|---|---|---|
| **Native Plugins** | Written directly against `zero-plugins` hooks (`onBeforeSend`, `start`, `stop`) | Custom message formatters, local automations | **Supported & Verified** |
| **Vencord (Adapter)** | Uses `definePlugin`, `definePluginSettings`, `OptionType`, `addMessagePreSendListener`, `onBeforeSend` without Webpack patches | `TextReplace`, `AutoCapitalize` | **Supported & Verified** (`zero-vencord`) |
| **Vencord (Unchanged)** | Pure JS plugins with zero Discord internals | Simple non-DOM message transforms | **Supported** (via `zero-vencord` shim) |
| **Requires Discord Internals** | Relies on `patches: [...]`, `@webpack`, `findByProps`, `FluxDispatcher`, or `React.*` | `ShowHiddenChannels`, `BetterFolders`, `Experiments` | **Blocked & Honestly Reported** |
| **CSS Themes** | `:root` custom properties (`--background-primary`, `--background-secondary`, `--background-tertiary`, `--channeltextarea-background`, `--text-normal`, `--text-muted`, `--brand-experiment`, `--font-size`) | Custom Discord/Vencord color themes | **Translated to `ThemeTokens`**; `@import`, selectors, and `backdrop-filter`/`transform` reported as unsupported |

## 2. Sandbox & Security Guarantees (`zero-plugins`)
- **Engine:** Pure-Rust `boa_engine` v0.20.
- **Lazy Initialization:** JS sandbox is **not** allocated until a plugin is explicitly enabled (`0 MB` overhead when disabled).
- **Resource Limits:** `JS_LOOP_ITERATION_LIMIT = 50,000`, `JS_RECURSION_LIMIT = 64`. Infinite loops and runtime exceptions are isolated into `PluginState::Failed(String)` without hanging or crashing the host.
- **Permission Boundaries:** Plugins can only request `MessageHooks`, `SettingsReadWrite`, and `Logging`. Filesystem, network, and OS credential access are not exposed to the JS realm.

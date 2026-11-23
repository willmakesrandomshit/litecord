# Litecord

**A native, ultra-lightweight Rust desktop communication client — zero Electron, zero Chromium, ~43–51 MB RAM.**

Litecord is an open-source, native-first desktop client inspired by Discord and engineered in pure Rust with `iced`, `tokio`, `rusqlite`, `reqwest`, `boa_engine`, and `cpal`. It is built to deliver responsive messaging, 100,000+ message timeline virtualization, Vencord-compatible JavaScript plugins, CSS variable themes, and native OS audio/screen capture at a fraction of Electron's memory footprint.

---

## Measured Performance (Windows x64 Release Build)

| Metric | Litecord (`tiny-skia` default) | Notes |
|---|---|---|
| **Idle RAM (Working Set)** | **43.18 – 45.79 MB** | Measured over 60s idle |
| **Private Memory (Idle)** | **16.24 – 16.54 MB** | — |
| **Warm Startup** | **32 – 63 ms** | Window ready (`MainWindowHandle`) |
| **Idle CPU** | **0.000 s delta** (3s -> 60s) | Near-zero background activity |
| **100,000 Virtualized Messages** | **42.23 MB** Working Set | `O(log N)` visible-range slicing (`TIMELINE_OVERSCAN = 6`) |
| **2 Active Vencord JS Plugins** | **50.81 MB** Working Set | Lazy `boa_engine` sandbox (`0 MB` when disabled) |

> `ICED_BACKEND=wgpu` is also supported as a runtime override if GPU rendering is preferred over `tiny-skia`.

---

## Key Features

### Messaging & Virtualization (`zero-core`, `zero-cache`, `zero-storage`)
- **Virtualized Message Timeline:** Prefix-sum height table with `O(log N)` visible-range queries, overscan, variable-height estimation, and stable scroll anchoring.
- **Full Local Messaging:** Replies, inline message editing, deletion, pinning + `📌 Pinned` filter, reactions, channel search, local channel creation (`+ new-channel`), and per-channel composer drafts.
- **Rich Text Rendering:** Native rendering for ` ```code blocks``` `, `` `inline code` ``, `@mentions`, and click-to-reveal `||spoilers||`.
- **SQLite Persistence:** WAL-mode `rusqlite` storage for settings and local message history.
- **Bounded Memory:** Byte-budget LRU cache (`zero-cache`) with hit/miss/eviction metrics.

### Vencord Compatibility & Themes (`zero-plugins`, `zero-vencord`, `zero-themes`)
- **Sandboxed JS Runtime:** Pure-Rust `boa_engine` v0.20 with lazy initialization, loop iteration limits (`50,000`), recursion limits (`64`), permission boundaries, and crash isolation.
- **Vencord Adapter (`zero-vencord`):** Supports `definePlugin`, `definePluginSettings`, `OptionType`, `onBeforeSend`, `@api/Commands` (`/shrug`, `/tableflip`, `/lenny`, `/mock`), and `FluxDispatcher` (`MESSAGE_CREATE`), plus custom plugin import in Settings.
- **Honest Webpack Boundary:** Static analyzer (`analyze_vencord_source`) detects `patches:`, `@webpack`, `findByProps`, and `React.` and flags plugins requiring Discord's Electron/Webpack internals rather than faking compatibility.
- **Theme Engine:** Built-in `Discord Zero Dark`, `Discord Zero Light`, and `Midnight OLED`, plus Discord/Vencord `:root` CSS variable import (`--background-primary`, `--accent`, etc.) with explicit reporting of unsupported CSS constructs.

### Networking & Media (`zero-network`, `zero-discord`, `zero-media`)
- **Official Discord API v10 Client:** Rate-limit-aware HTTP scheduler (`X-RateLimit-*` + `429` backoff), exponential retry, diagnostics, and OS Credential Manager storage (`keyring`) for authorized Bot tokens.
- **Live Verified:** `GET https://discord.com/api/v10/gateway` verified against `discord.com`.
- **Native Audio & Screen Capture:** `cpal` (WASAPI/CoreAudio/ALSA) device enumeration, 440 Hz output test chime, live microphone RMS level monitor, Mute/Deafen/Push-to-Talk, and `screenshots` display capture.

> **Security & Policy Boundary:** Litecord never scrapes user tokens, bypasses authentication, or spoofs official Discord client-integrity signals. Normal user-account Gateway emulation is intentionally unsupported; live Discord connectivity uses official Bot/App/Webhook APIs.

---

## Workspace Architecture

```text
litecord/
├── apps/desktop/          # Native iced GUI binary (`litecord`)
├── crates/
│   ├── zero-core/         # Domain models, state, markdown parser, VirtualTimeline
│   ├── zero-cache/        # Byte-budget LRU & lazy image cache
│   ├── zero-storage/      # SQLite persistence (rusqlite WAL)
│   ├── zero-network/      # Rate-limited HTTP client, backoff, diagnostics, OS keyring
│   ├── zero-discord/      # Typed Discord API v10 REST client
│   ├── zero-plugins/      # Sandboxed boa_engine JS plugin runtime
│   ├── zero-vencord/      # Vencord definePlugin / Commands / FluxDispatcher adapter
│   ├── zero-themes/       # Theme manager & CSS variable translator
│   └── zero-media/        # Native audio I/O (cpal) & screen capture (screenshots)
├── docs/                  # Architecture, feature matrix, benchmarks, security, state
└── scripts/               # Reproducible memory & startup benchmark scripts
```

---

## Build & Run

```bash
cargo test --workspace
cargo build --release -p litecord
.\target\release\litecord.exe
```

Optional CLI flags:
- `--synthetic 100000` — populate 100,000 synthetic messages to test timeline virtualization.
- `--enable-plugin vencord.text-replace` — enable a plugin at startup.

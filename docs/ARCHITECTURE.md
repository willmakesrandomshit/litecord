# Discord Zero — Architecture & Decision Record

## 1. GUI Framework Decision Record: `iced` vs `Slint`

**Decision:** Select **`iced`** (v0.13+) as the primary native GUI framework.

### Evaluation Criteria

| Criterion | `iced` | `Slint` |
|---|---|---|
| Language & Ecosystem | 100% pure Rust; seamless sharing of `zero-core` models and `tokio` streams | `.slint` DSL + Rust bridge; extra boundary for dynamic message layouts |
| Rendering & GPU | `wgpu` (DX12, Vulkan, Metal) + `tiny-skia` software fallback | FemtoVG / Skia / Software |
| Memory Overhead | Minimal; no embedded JS/QML/C++ runtime | Low, though Skia backend increases binary size & memory |
| Text Shaping & Rich Text | Built on `cosmic-text` / `glyphon`; native `rich_text` & `markdown` support | Good basic text, but complex inline Discord markdown/code/mentions are harder in `.slint` |
| Virtualized Lists | Viewport-windowed rendering via scrollable offset tracking + cached row heights | Built-in `ListView` virtualization |
| Async Integration | First-class `iced::Task` and `iced::Subscription` backed by `tokio` | Manual `slint::invoke_from_event_loop` |
| Licensing | MIT (fully permissive) | GPLv3 / Royalty-free attribution |

**Conclusion:** `iced` provides the cleanest pure-Rust architecture, lowest runtime/memory footprint, superior `cosmic-text` shaping for chat-heavy UIs, and native `tokio` integration.

## 2. Workspace Structure

```text
discord-zero/
├── Cargo.toml                  # Workspace root
├── apps/
│   └── desktop/                # Native iced desktop application (`discord-zero`)
├── crates/
│   ├── zero-core/              # Domain models (Guild, Channel, Message, User), state, demo data
│   ├── zero-storage/           # SQLite persistence (settings, local message store, cache index)
│   ├── zero-cache/             # Bounded LRU caches for messages, users, and media metadata
│   ├── zero-network/           # (Phase C) HTTP/WebSocket transport abstractions
│   ├── zero-discord/           # (Phase C) Supported Discord REST/Gateway integration
│   ├── zero-plugins/           # (Phase D) Sandboxed plugin runtime & Vencord compatibility layer
│   ├── zero-media/             # (Phase E) Local audio/video capture & playback subsystems
│   └── zero-benchmarks/        # Criterion & memory benchmarks
└── docs/                       # Architecture, feature matrix, performance, security, current state
```

Only crates with immediate architectural value are created upfront; additional crates are added as their phase begins.

## 3. Core Design Principles

1. **Independence of UI & Connectivity:** `zero-core` models messages, guilds, channels, and state transitions independently of Discord's network layer.
2. **Bounded Memory:** All caches use explicit capacity limits and LRU eviction.
3. **Zero Unsafe by Default:** `#![forbid(unsafe_code)]` where possible; any FFI/platform exception must be isolated and documented.
4. **Security & Compliance:** Official/supported auth and API boundaries only; no token scraping, client-integrity spoofing, or rate-limit evasion.

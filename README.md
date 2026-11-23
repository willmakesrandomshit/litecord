# Discord Zero

A lightweight, native Rust desktop communication client built for extreme resource efficiency, fast startup, and extensibility.

## Architecture

- **GUI:** `iced` (native GPU-accelerated `wgpu` + `tiny-skia` fallback, pure Rust)
- **Core:** `zero-core` (domain models, application state, bounded message timeline)
- **Storage:** `zero-storage` (SQLite via `rusqlite` for settings and local persistence)
- **Desktop App:** `apps/desktop` (`discord-zero`)

## Building & Running

```bash
cargo test --workspace
cargo run -p discord-zero
```

See `docs/CURRENT_STATE.md` and `docs/ARCHITECTURE.md` for current progress and design decisions.

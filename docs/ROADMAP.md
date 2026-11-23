# Discord Zero — Development Roadmap

- **Phase A — Foundation (IN PROGRESS):** Cargo workspace, `iced` native desktop shell, `zero-core` domain state, `zero-storage` SQLite persistence, server/channel navigation, message timeline, local composer, settings persistence, automated unit tests, baseline memory measurement.
- **Phase B — Performance Core:** Virtualized message timeline (10,000+ messages), bounded LRU caches (`zero-cache`), lazy image/attachment decoding, Criterion benchmarks (`zero-benchmarks`).
- **Phase C — Discord Integrations:** Supported authentication & API integration, `zero-network` / `zero-discord`, reconnection & rate-limit handling, live-verified integration tests.
- **Phase D — Customization:** Theme manager (custom themes/CSS subset), sandboxed JS plugin runtime, Vencord compatibility adapters & tested plugins.
- **Phase E — Media:** Local audio capture/playback (`zero-media`), device selection, mic/speaker tests, video/screen capture prototype.
- **Phase F — Feature Expansion:** Markdown/code blocks/spoilers, replies, threads, pinned messages, rich embeds.
- **Phase G — Release:** Windows/macOS/Linux builds, CI workflows, performance & security reports.

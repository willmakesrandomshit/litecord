# Discord Zero — Feature Parity Matrix

States: `NOT STARTED` | `RESEARCHING` | `IMPLEMENTING` | `MOCK VERIFIED` | `INTEGRATION VERIFIED` | `LIVE VERIFIED` | `BLOCKED` | `UNSUPPORTED`

## Capability Boundaries
- **Supported Live Discord APIs:** Official Discord API v10 (`Bot` / `App` / `Webhook` / `GET /gateway`).
- **Unsupported for Normal User Accounts:** Third-party user-account token scraping, unofficial client-integrity spoofing, and user-Gateway emulation are intentionally `UNSUPPORTED`.

## Messaging
| Feature | State | Notes |
|---|---|---|
| Discord API v10 Gateway check (`GET /gateway`) | LIVE VERIFIED | Live HTTPS test against `discord.com` passes (`0.24s`) |
| Bot API: Guilds / Channels / Messages / Send | INTEGRATION VERIFIED | Typed REST v10 client + non-blocking UI wiring; requires authorized Bot token |
| Webhook message posting | IMPLEMENTING | `DiscordClient::execute_webhook` implemented |
| Server navigation | MOCK VERIFIED | Local demo + Bot guild list |
| Text channels | MOCK VERIFIED | Local demo + Bot channel list |
| Direct messages | MOCK VERIFIED | Local demo; Bot DM creation not yet wired |
| Group messages | UNSUPPORTED | Restricted for third-party user clients |
| Message sending (local) | MOCK VERIFIED | Persists to SQLite |
| Message editing / deletion | NOT STARTED | |
| Replies / Mentions | NOT STARTED | |
| Reactions (local) | MOCK VERIFIED | Interactive toggle in timeline |
| Threads / Pinned messages | NOT STARTED | |
| Message search (local) | MOCK VERIFIED | Case-insensitive channel search |
| Markdown / Code blocks / Spoilers | NOT STARTED | |
| Timestamps | MOCK VERIFIED | Local timezone formatting |
| Custom emoji / Stickers / GIFs | NOT STARTED | |
| Attachments / Image & File previews | NOT STARTED | |
| Unread indicators | MOCK VERIFIED | Cleared on channel selection |
| Message history & pagination | MOCK VERIFIED | `VirtualTimeline` verified up to 100k synthetic msgs |
| Virtualized message timeline | MOCK VERIFIED | `O(log N)` visible range + overscan + scroll anchoring |
| Bounded LRU & lazy image cache | MOCK VERIFIED | `zero-cache` byte-budget LRU + metrics |

## Security & Networking
| Feature | State | Notes |
|---|---|---|
| Rate-limit-aware HTTP scheduler | MOCK VERIFIED | Per-route buckets + global limit + 429 backoff (`zero-network`) |
| Exponential reconnection backoff | MOCK VERIFIED | Unit tested in `zero-network` |
| OS Credential Store (`keyring`) | INTEGRATION VERIFIED | Windows Credential Manager / Keychain / Secret Service |
| Network Diagnostics UI | MOCK VERIFIED | Live state, request counts, rate-limit hits, latency |

## Customization & Media
| Feature | State | Notes |
|---|---|---|
| Theme manager (Dark / Light / Midnight OLED / CSS import) | MOCK VERIFIED | `zero-themes` translates CSS vars to `ThemeTokens`; reports unsupported CSS |
| Sandboxed JS plugin runtime (`zero-plugins`) | MOCK VERIFIED | Lazy `boa_engine` sandbox, loop/recursion limits, crash isolation, permissions |
| Vencord compatibility adapter (`zero-vencord`) | MOCK VERIFIED | `definePlugin` / `definePluginSettings` / `onBeforeSend`; Webpack plugins honestly blocked |
| Voice & Video (Discord Voice Gateway) | NOT STARTED | Local OS audio I/O is LIVE VERIFIED; Discord UDP/Opus voice not yet implemented |
| Local Audio I/O (`zero-media`) | LIVE VERIFIED | `cpal` device enumeration, mic RMS monitor, 440Hz output chime, Mute/Deafen/PTT |

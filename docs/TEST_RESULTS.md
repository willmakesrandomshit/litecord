# Discord Zero — Test Results

## 2026-11-22 — Phase D Verification
Commands:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings)
- `cargo test --workspace`

Results (**26 passed, 0 failed**):
- `zero-core` (**9 passed, 0 failed**)
- `zero-cache` (**3 passed, 0 failed**)
- `zero-storage` (**2 passed, 0 failed**)
- `zero-network` (**3 passed, 0 failed**)
- `zero-discord` (**2 passed, 0 failed**, including live `GET /api/v10/gateway`)
- `zero-plugins` (**3 passed, 0 failed**):
  - `tests::sandbox_is_lazy_until_plugin_enabled`
  - `tests::infinite_loop_in_plugin_is_isolated_without_hanging_host`
  - `tests::permission_boundary_blocks_unpermitted_message_hook`
- `zero-themes` (**2 passed, 0 failed**):
  - `tests::css_theme_parser_maps_discord_variables_and_reports_unsupported`
  - `tests::theme_manager_installs_exports_and_removes_custom_themes`
- `zero-vencord` (**2 passed, 0 failed**):
  - `tests::vencord_adapter_runs_text_replace_and_respects_settings`
  - `tests::webpack_dependent_plugin_is_honestly_blocked`

Release smoke & plugin overhead: `0` plugins `43.18 MB` WS -> `1` plugin `50.50 MB` WS -> `2` plugins `50.81 MB` WS.

## 2026-11-22 — Phase C Verification
Commands:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings)
- `cargo test --workspace`

Results (**19 passed, 0 failed**):
- `zero-core` (**9 passed, 0 failed**): state, bounded queue, reactions, search, `VirtualTimeline` 100k range, scroll anchor, variable height.
- `zero-cache` (**3 passed, 0 failed**): byte-budget LRU eviction, oversize rejection, lazy image state.
- `zero-storage` (**2 passed, 0 failed**): SQLite settings & message roundtrips.
- `zero-network` (**3 passed, 0 failed**):
  - `credentials::tests::memory_credential_store_roundtrip`
  - `rate_limit::tests::exponential_backoff_caps_at_max`
  - `rate_limit::tests::rate_limiter_updates_and_waits_on_exhausted_bucket`
- `zero-discord` (**2 passed, 0 failed**):
  - `models::tests::discord_message_deserializes_and_maps_to_zero_core`
  - `live_discord_api_gateway_endpoint_responds` (**LIVE VERIFIED** against `https://discord.com/api/v10/gateway` in `0.25s`)

## Phase B Verification & Phase C Post-Networking Smoke Test
- **Multi-launch (Phase B):** Run 1 `1615ms` (cold disk) / Run 2 `32ms` / Run 3 `46ms`; `42.23–42.63 MB` WS.
- **100k synthetic scroll test:** `alive=True`, `WS_BEFORE=69.11MB`, `WS_AFTER=69.11MB`, `CPU_DELTA=0s` across 30 `WM_MOUSEWHEEL` events.
- **Phase C Release Smoke Test (post-networking):** `alive=True`, binary `16.05 MB`, Working Set `43.21 MB`, Private Memory `16.54 MB`, CPU `0.125 s`.

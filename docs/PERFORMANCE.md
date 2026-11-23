# Discord Zero — Performance & Benchmarks

## Stretch Targets vs. Measured Phase B Results

| Measurement | Stretch Goal | Phase B Measured (`tiny-skia` default) | `wgpu` Comparison |
|---|---|---|---|
| Idle Working Set (RAM) | < 75 MB | **45.79 MB** (60s idle) | 178.80 MB (30s idle) |
| Idle Private Memory | — | **16.24 MB** (60s idle) | 205.11 MB (30s idle) |
| Cold Startup (window ready) | < 1,000 ms | **55 ms** | 1,283 ms |
| Idle CPU (3s -> 60s delta) | Near 0% | **0.000 s** (0.141 s total) | 0.047 s delta (1.000 s total at 30s) |
| 100,000 Messages Virtualized | < 150 MB | **42.23 MB WS / 16.04 MB Priv** | 179.19 MB WS / 204.48 MB Priv |
| Executable Size (Release) | — | **12.94 MB** | 12.94 MB |

## Renderer Decision: `tiny-skia` Default (with `ICED_BACKEND=wgpu` Override)
- **Measured Evidence (`scripts/bench_memory.ps1`):**
  - `idle_wgpu`: startup `1283ms`, 3s `WS=178.93MB / PRIV=205.24MB / CPU=0.953s`, 10s `WS=178.81MB / PRIV=205.14MB / CPU=0.953s`, 30s `WS=178.8MB / PRIV=205.11MB / CPU=1s`
  - `idle_tiny_skia`: startup `55ms`, 3s `WS=45.82MB / PRIV=16.37MB / CPU=0.141s`, 10s `WS=45.82MB / PRIV=16.37MB / CPU=0.141s`, 30s `WS=45.83MB / PRIV=16.37MB / CPU=0.141s`, 60s `WS=45.79MB / PRIV=16.24MB / CPU=0.141s`
  - `synth_100` (`tiny-skia`): startup `45ms`, 3s `WS=42.61MB / PRIV=16.59MB / CPU=0.141s`
  - `synth_1k` (`tiny-skia`): startup `44ms`, 3s `WS=42.5MB / PRIV=16.36MB / CPU=0.109s`
  - `synth_10k` (`tiny-skia`): startup `45ms`, 3s `WS=42.49MB / PRIV=16.39MB / CPU=0.141s`
  - `synth_100k` (`tiny-skia`): startup `46ms`, 3s `WS=42.23MB / PRIV=16.04MB / CPU=0.156s`
  - `synth_100k_wgpu` (`wgpu`): startup `45ms` (warm), 3s `WS=179.19MB / PRIV=204.48MB / CPU=0.797s`
- **Conclusion:** `tiny-skia` achieves ~4x lower Working Set, ~12x lower Private Memory, 23x faster cold startup, and 0.000s idle CPU delta over 60 seconds. `discord-zero` defaults `ICED_BACKEND` to `tiny-skia` while preserving `ICED_BACKEND=wgpu` as a runtime override.

## Virtualization & Cache Architecture
- `VirtualTimeline` (`crates/zero-core/src/virtual_list.rs`): prefix-sum height table with `O(log N)` visible-range lookup, `O(1)` incremental append, `TIMELINE_OVERSCAN = 6`, and `ScrollAnchor` tracking.
- `zero-cache` (`crates/zero-cache/src/lib.rs`): `BoundedCache` (item count + byte-budget LRU eviction, hit/miss/eviction metrics) and `ImageCache` (`Unloaded`/`Decoding`/`Ready`/`Failed` lazy state).

## Phase D — Plugin & Theme Overhead (Release Build, Windows x64)
Measured via `target/release/discord-zero.exe` (`26.62 MB` with `boa_engine`):
- **0 plugins (sandbox unloaded):** `WS=43.18 MB` | `PRIV=16.52 MB` | `CPU=0.109 s`
- **1 plugin (`vencord.text-replace`):** `startup=61 ms` | `WS=50.50 MB` (+7.32 MB) | `PRIV=23.94 MB` | `CPU=0.125 s`
- **2 plugins (`vencord.text-replace` + `vencord.auto-capitalize`):** `startup=63 ms` | `WS=50.81 MB` (+0.31 MB) | `PRIV=24.22 MB` | `CPU=0.141 s`

# Framework Benchmark Summary

Generated: 2026-06-25T06:24:49.572496+00:00

Raw run: `results/raw/20260625T054533Z`

## Methodology

| key | value |
| --- | --- |
| machine | macOS-26.2-arm64-arm-64bit-Mach-O |
| python | Python 3.14.2 |
| rust | rustc 1.91.1 (ed61e7d7e 2025-11-07) |
| cargo | cargo 1.91.1 (ea2d97820 2025-10-10) |
| duration | 3.0s per scenario |
| warmup | 1.0s before measured scenarios |
| concurrency | [1, 16, 64, 256] |
| load generator | repository stdlib HTTP runner, one connection per request |
| run count | one baseline run per scenario |

## Single-File LOC

| implementation | loc |
| --- | ---: |
| `src/bin/t1_vyuh.rs` | 67 |
| `src/bin/t1_axum.rs` | 83 |
| `src/bin/t1_rocket.rs` | 76 |
| `src/bin/t1_actix.rs` | 83 |
| `python/t1_fastapi.py` | 33 |
| `src/bin/t2_vyuh.rs` | 189 |
| `src/bin/t2_axum.rs` | 199 |
| `src/bin/t2_rocket.rs` | 192 |
| `src/bin/t2_actix.rs` | 201 |
| `python/t2_fastapi.py` | 85 |
| `python/drf_t1/` | 50 |
| `python/drf_t2/` | 114 |

### T1 c64

| framework | health | echo | item | errors |
| --- | ---: | ---: | ---: | ---: |
| vyuh | 8713 rps / p99 14.9ms | 9137 rps / p99 8.2ms | 9000 rps / p99 6.6ms | 0 |
| axum | 9122 rps / p99 6.4ms | 9087 rps / p99 6.5ms | 8921 rps / p99 7.2ms | 0 |
| rocket | 9089 rps / p99 7.0ms | 9050 rps / p99 6.5ms | 8972 rps / p99 7.7ms | 0 |
| actix | 9090 rps / p99 7.5ms | 9445 rps / p99 6.8ms | 8922 rps / p99 12.3ms | 0 |
| fastapi | 4319 rps / p99 34.5ms | 3734 rps / p99 33.1ms | 2338 rps / p99 42.7ms | 0 |
| drf | 1390 rps / p99 63.9ms | 1286 rps / p99 59.4ms | 1153 rps / p99 76.3ms | 0 |

### T1 c256

| framework | health | echo | item | errors |
| --- | ---: | ---: | ---: | ---: |
| vyuh | 9257 rps / p99 52.4ms | 8971 rps / p99 62.3ms | 8840 rps / p99 65.1ms | 0 |
| axum | 9169 rps / p99 45.4ms | 9104 rps / p99 51.1ms | 9216 rps / p99 48.0ms | 0 |
| rocket | 9129 rps / p99 54.4ms | 9157 rps / p99 50.9ms | 3574 rps / p99 348.8ms | 0 |
| actix | 9285 rps / p99 58.6ms | 9064 rps / p99 51.8ms | 8430 rps / p99 58.0ms | 0 |
| fastapi | 4425 rps / p99 604.5ms | 4036 rps / p99 611.5ms | 2449 rps / p99 1172.5ms | 0 |
| drf | 1440 rps / p99 4000.3ms | 1372 rps / p99 4018.0ms | 1215 rps / p99 4020.5ms | 0 |

### T2 c64

| framework | health | summary | events_read | events_write | errors |
| --- | ---: | ---: | ---: | ---: | ---: |
| vyuh | 2549 rps / p99 2022.8ms | 3551 rps / p99 18.3ms | 2549 rps / p99 2023.4ms | 3499 rps / p99 29.3ms | 0 |
| axum | 3273 rps / p99 31.2ms | 2552 rps / p99 2019.1ms | 3624 rps / p99 20.2ms | 2492 rps / p99 2022.0ms | 0 |
| rocket | 2552 rps / p99 2020.6ms | 3636 rps / p99 25.1ms | 2556 rps / p99 2017.4ms | 3631 rps / p99 12.2ms | 0 |
| actix | 4213 rps / p99 2014.3ms | 2551 rps / p99 2019.1ms | 3831 rps / p99 8.6ms | 2562 rps / p99 2018.7ms | 0 |
| fastapi | 2556 rps / p99 2023.6ms | 1609 rps / p99 2020.9ms | 1750 rps / p99 2019.0ms | 2345 rps / p99 2012.7ms | 0 |
| drf | 838 rps / p99 2015.7ms | 283 rps / p99 2181.0ms | 264 rps / p99 345.5ms | 253 rps / p99 323.9ms | 0 |

### T2 c256

| framework | health | summary | events_read | events_write | errors |
| --- | ---: | ---: | ---: | ---: | ---: |
| vyuh | 9052 rps / p99 46.4ms | 8862 rps / p99 45.4ms | 8990 rps / p99 39.3ms | 8368 rps / p99 41.1ms | 0 |
| axum | 9135 rps / p99 34.8ms | 8452 rps / p99 63.4ms | 8816 rps / p99 36.1ms | 8029 rps / p99 2014.7ms | 0 |
| rocket | 9039 rps / p99 51.3ms | 8826 rps / p99 37.3ms | 8949 rps / p99 40.3ms | 8238 rps / p99 51.6ms | 0 |
| actix | 9148 rps / p99 51.0ms | 9036 rps / p99 43.2ms | 8992 rps / p99 39.4ms | 8196 rps / p99 41.8ms | 0 |
| fastapi | 7721 rps / p99 2013.2ms | 4809 rps / p99 2015.0ms | 4520 rps / p99 2015.1ms | 4105 rps / p99 2016.1ms | 0 |
| drf | 1377 rps / p99 3993.9ms | 357 rps / p99 4378.2ms | 333 rps / p99 4445.8ms | 310 rps / p99 4457.0ms | 0 |

## Dev UX Measures

Dependency count is the number of external package roots imported by the implementation source, excluding standard-library imports. It is not transitive dependency count.

LOC excludes blank lines and comments only. It still includes DTOs, derives, route handlers, SQL query strings, startup, and runtime plumbing. Shared schema SQL is excluded when it lives under `sql/`.

| target | files | LOC | direct imported deps | deps |
| --- | ---: | ---: | ---: | --- |
| t1/vyuh | 1 | 67 LOC / 575 tokens | 2 | `sqlx`, `vyuh` |
| t1/axum | 1 | 83 LOC / 620 tokens | 3 | `axum`, `serde`, `sqlx` |
| t1/rocket | 1 | 76 LOC / 587 tokens | 2 | `rocket`, `sqlx` |
| t1/actix | 1 | 83 LOC / 615 tokens | 3 | `actix_web`, `serde`, `sqlx` |
| t1/fastapi | 1 | 33 LOC / 265 tokens | 2 | `fastapi`, `pydantic` |
| t1/drf | 5 folder files | 50 LOC / 369 tokens | 2 | `django`, `rest_framework` |
| t2/vyuh | 1 | 189 LOC / 1485 tokens | 3 | `serde_json`, `sqlx`, `vyuh` |
| t2/axum | 1 | 199 LOC / 1587 tokens | 7 | `axum`, `futures_util`, `serde`, `serde_json`, `sqlx`, `tokio`, `tokio_stream` |
| t2/rocket | 1 | 192 LOC / 1415 tokens | 3 | `rocket`, `sqlx`, `tokio` |
| t2/actix | 1 | 201 LOC / 1503 tokens | 6 | `actix_web`, `async_stream`, `serde`, `serde_json`, `sqlx`, `tokio` |
| t2/fastapi | 1 | 85 LOC / 755 tokens | 4 | `asyncpg`, `fastapi`, `pydantic`, `sse_starlette` |
| t2/drf | 5 folder files | 114 LOC / 920 tokens | 3 | `django`, `psycopg`, `rest_framework` |

## Tier 2 Runtime Plumbing

| framework | scheduler/rollup code | live delivery code | app-facing shape |
| --- | --- | --- | --- |
| vyuh | `#[bundles::periodic]` emits typed tick into `#[bundles::signal]` | `ChannelRef.publish`, `ChannelRef.sse`, `ChannelRef.long_poll` | runtime surfaces are registered and introspectable |
| axum | explicit `tokio::spawn` interval loop | explicit `broadcast::Sender`, SSE stream filter, long-poll loop | manual app state and plumbing |
| rocket | explicit spawned interval loop | explicit broadcast channel plus `EventStream` route | manual managed state and plumbing |
| actix | explicit spawned interval loop | explicit broadcast channel plus streaming response | manual app state and plumbing |
| fastapi | explicit `asyncio.create_task` rollup loop | explicit per-project queue set plus `sse-starlette` | manual queues and task lifecycle |
| drf | explicit background thread rollup loop | explicit event loop thread, queues, streaming response | folder-shaped Django app plus manual threads |


## Memory Snapshot

RSS is summed over the server process tree where available. Values are from this single run.

| target | idle RSS MiB | max after-scenario RSS MiB |
| --- | ---: | ---: |
| t1/vyuh | 13.8 | 23.5 |
| t1/axum | 11.2 | 17.0 |
| t1/rocket | 10.5 | 21.3 |
| t1/actix | 12.0 | 20.3 |
| t1/fastapi | 59.0 | 280.7 |
| t1/drf | 91.3 | 194.6 |
| t2/vyuh | 13.5 | 31.0 |
| t2/axum | 11.7 | 20.9 |
| t2/rocket | 11.1 | 23.4 |
| t2/actix | 14.3 | 25.5 |
| t2/fastapi | 60.7 | 67.8 |
| t2/drf | 84.5 | 98.0 |

## Rust Binary Size

| binary | size MiB |
| --- | ---: |
| t1_vyuh | 18.9 |
| t1_axum | 8.1 |
| t1_rocket | 7.3 |
| t1_actix | 8.3 |
| t2_vyuh | 18.5 |
| t2_axum | 7.2 |
| t2_rocket | 6.3 |
| t2_actix | 7.3 |

## Capability Matrix

| capability | vyuh | axum | rocket | actix | fastapi | drf |
| --- | --- | --- | --- | --- | --- | --- |
| scheduler/cron | built-in emitters | tokio task | spawned task | runtime task | app task/add-on | thread/add-on |
| live delivery | built-in channels | broadcast/SSE | stream/SSE | stream/SSE | add-on/SSE | streaming/thread |
| integrated OpenAPI | built-in | add-on | add-on | add-on | built-in | add-on |
| auth/middleware metadata | integrated | manual/add-on | manual/add-on | manual/add-on | partial | add-on |
| operation introspection | built-in console | manual | manual | manual | partial | partial |
| background service model | built-in | manual | manual | manual | manual | manual |
| implementation shape | single file | single file | single file | single file | single file | folder-shaped |


## Interpretation

This run is useful as a reproducible baseline and harness validation, not as a final benchmark claim. It is a single run per scenario using the repository's standard-library HTTP load runner, which opens one connection per request. Several Tier 2 rows show around 2s p99 outliers across multiple frameworks, so p99 should not be used for ranking until repeated median runs are collected with a stronger load generator.

What the current data does support:

- Vyuh Tier 1 is in the same broad performance band as Axum for typed handlers. At c64, Vyuh echo is `9137 rps` versus Axum `9087 rps`; Vyuh item lookup is `9000 rps` versus Axum `8921 rps`.
- All 168 measured scenario rows completed with zero HTTP-status errors in success scenarios.
- Python stacks are materially lower in this run. At Tier 2 c256 writes, Vyuh reports `8368 rps`; DRF reports `310 rps`.
- Tier 2 implementation ergonomics remain the more important Vyuh strength: scheduled rollup, signal fanout, channel publish, SSE, long-poll, and operation metadata are registered runtime surfaces instead of separately assembled background/pubsub machinery.

Raw data: `results/raw/20260625T054533Z/`.


## Limitations

- This is not yet a five-run median with min/max.
- The standard-library runner is intentionally dependency-free but is not a replacement for `oha`, `wrk`, or `bombardier`.
- The p99 values in Tier 2 contain repeated multi-second outliers across frameworks, so they are recorded but not used for ranking.
- SSE/long-poll correctness routes exist in the apps, but this run measured HTTP read/write endpoints only; live subscriber fanout still needs a dedicated scenario.
- Startup readiness is not reported for this raw run because the first harness version recorded it after readiness completed; the harness is fixed for future runs.
- Python apps ran single worker/process baselines, matching the single-process Rust baseline.

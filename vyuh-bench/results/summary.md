# Framework Benchmark Summary

Generated: 2026-06-28T20:59:40.624693+00:00

Raw run: `results/raw/20260628T204547Z`

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
| `src/bin/t1_vyuh.rs` | 51 |
| `src/bin/t1_axum.rs` | 64 |
| `src/bin/t1_rocket.rs` | 59 |
| `src/bin/t1_actix.rs` | 63 |
| `python/t1_fastapi.py` | 31 |
| `src/bin/t2_vyuh.rs` | 202 |
| `src/bin/t2_axum.rs` | 226 |
| `src/bin/t2_rocket.rs` | 218 |
| `src/bin/t2_actix.rs` | 233 |
| `python/t2_fastapi.py` | 103 |
| `python/drf_t1/` | 50 |
| `python/drf_t2/` | 114 |

### T1 c64

| framework | health | echo | item | errors |
| --- | ---: | ---: | ---: | ---: |
| vyuh | 8950 rps / p99 7.4ms | 8799 rps / p99 7.0ms | 8611 rps / p99 7.0ms | 0 |
| axum | 8850 rps / p99 6.9ms | 8855 rps / p99 6.9ms | 8491 rps / p99 7.2ms | 0 |
| rocket | 8485 rps / p99 7.9ms | 8538 rps / p99 7.9ms | 8019 rps / p99 15.2ms | 0 |
| actix | 9295 rps / p99 7.2ms | 8765 rps / p99 8.4ms | 8408 rps / p99 13.2ms | 0 |
| fastapi | 3968 rps / p99 37.3ms | 3693 rps / p99 28.6ms | 2154 rps / p99 79.4ms | 0 |
| drf | 1411 rps / p99 56.9ms | 1384 rps / p99 54.5ms | 1160 rps / p99 88.8ms | 0 |

### T1 c256

| framework | health | echo | item | errors |
| --- | ---: | ---: | ---: | ---: |
| vyuh | 8655 rps / p99 55.7ms | 8533 rps / p99 50.6ms | 8339 rps / p99 64.6ms | 0 |
| axum | 8663 rps / p99 53.0ms | 8774 rps / p99 28.2ms | 8288 rps / p99 47.2ms | 0 |
| rocket | 8645 rps / p99 53.6ms | 8744 rps / p99 44.8ms | 8268 rps / p99 55.6ms | 0 |
| actix | 9037 rps / p99 55.4ms | 8704 rps / p99 58.0ms | 8535 rps / p99 59.3ms | 0 |
| fastapi | 4051 rps / p99 605.8ms | 3925 rps / p99 607.4ms | 2448 rps / p99 1056.4ms | 0 |
| drf | 1484 rps / p99 3992.2ms | 1408 rps / p99 3937.4ms | 1251 rps / p99 4019.6ms | 0 |

### T2 c64

| framework | health | summary | events_read | events_write | errors |
| --- | ---: | ---: | ---: | ---: | ---: |
| vyuh | 3000 rps / p99 6.8ms | 5435 rps / p99 6.6ms | 5442 rps / p99 5.5ms | 5442 rps / p99 12.2ms | 0 |
| axum | 5440 rps / p99 6.3ms | 5439 rps / p99 11.1ms | 5449 rps / p99 5.4ms | 5440 rps / p99 14.5ms | 0 |
| rocket | 5442 rps / p99 4.7ms | 5450 rps / p99 5.7ms | 5452 rps / p99 10.6ms | 5453 rps / p99 15.5ms | 0 |
| actix | 5580 rps / p99 4.5ms | 5448 rps / p99 10.4ms | 5453 rps / p99 8.3ms | 5510 rps / p99 9.3ms | 0 |
| fastapi | 5473 rps / p99 8.1ms | 3777 rps / p99 30.4ms | 1613 rps / p99 2031.3ms | 3552 rps / p99 18.8ms | 0 |
| drf | 843 rps / p99 2044.0ms | 299 rps / p99 239.2ms | 297 rps / p99 1254.8ms | 283 rps / p99 283.5ms | 0 |

### T2 c256

| framework | health | summary | events_read | events_write | errors |
| --- | ---: | ---: | ---: | ---: | ---: |
| vyuh | 8918 rps / p99 42.1ms | 8792 rps / p99 48.4ms | 8890 rps / p99 38.8ms | 9215 rps / p99 46.4ms | 0 |
| axum | 9101 rps / p99 46.7ms | 8750 rps / p99 54.2ms | 8780 rps / p99 47.7ms | 9226 rps / p99 46.2ms | 0 |
| rocket | 8779 rps / p99 48.8ms | 8758 rps / p99 49.6ms | 8722 rps / p99 47.9ms | 8985 rps / p99 48.2ms | 0 |
| actix | 9414 rps / p99 46.2ms | 8792 rps / p99 48.8ms | 8454 rps / p99 51.1ms | 9269 rps / p99 47.2ms | 0 |
| fastapi | 7854 rps / p99 348.3ms | 4601 rps / p99 1082.2ms | 4228 rps / p99 2021.8ms | 4102 rps / p99 2015.7ms | 0 |
| drf | 1504 rps / p99 3968.8ms | 367 rps / p99 4412.6ms | 352 rps / p99 4330.5ms | 334 rps / p99 4413.4ms | 0 |

## Dev UX Measures

Dependency count is the number of external package roots imported by the implementation source, excluding standard-library imports. It is not transitive dependency count.

LOC excludes blank lines and comments only. It still includes DTOs, derives, route handlers, SQL query strings, startup, and runtime plumbing. Shared schema SQL is excluded when it lives under `sql/`.

| target | files | LOC | direct imported deps | deps |
| --- | ---: | ---: | ---: | --- |
| t1/vyuh | 1 | 51 LOC / 483 tokens | 1 | `vyuh` |
| t1/axum | 1 | 64 LOC / 506 tokens | 3 | `axum`, `serde`, `sqlx` |
| t1/rocket | 1 | 59 LOC / 481 tokens | 2 | `rocket`, `sqlx` |
| t1/actix | 1 | 63 LOC / 495 tokens | 3 | `actix_web`, `serde`, `sqlx` |
| t1/fastapi | 1 | 31 LOC / 216 tokens | 2 | `fastapi`, `pydantic` |
| t1/drf | 5 folder files | 50 LOC / 369 tokens | 2 | `django`, `rest_framework` |
| t2/vyuh | 1 | 202 LOC / 1549 tokens | 1 | `vyuh` |
| t2/axum | 1 | 226 LOC / 1559 tokens | 6 | `axum`, `futures_util`, `serde`, `sqlx`, `tokio`, `tokio_stream` |
| t2/rocket | 1 | 218 LOC / 1463 tokens | 3 | `rocket`, `sqlx`, `tokio` |
| t2/actix | 1 | 233 LOC / 1485 tokens | 5 | `actix_web`, `async_stream`, `serde`, `sqlx`, `tokio` |
| t2/fastapi | 1 | 103 LOC / 810 tokens | 4 | `asyncpg`, `fastapi`, `pydantic`, `sse_starlette` |
| t2/drf | 5 folder files | 114 LOC / 920 tokens | 3 | `django`, `psycopg`, `rest_framework` |

## Charts

Reusable assets are written under `results/charts/<raw-run>/` and mirrored to `results/charts/latest/`.

| chart | SVG | PNG |
| --- | --- | --- |
| Tier 1 Echo Throughput | [svg](results/charts/20260628T204547Z/t1-echo-c64-rps.svg) | [png](results/charts/20260628T204547Z/t1-echo-c64-rps.png) |
| Tier 2 Write Throughput | [svg](results/charts/20260628T204547Z/t2-write-c256-rps.svg) | [png](results/charts/20260628T204547Z/t2-write-c256-rps.png) |
| Tier 2 Peak Memory | [svg](results/charts/20260628T204547Z/t2-peak-rss-mib.svg) | [png](results/charts/20260628T204547Z/t2-peak-rss-mib.png) |
| Tier 2 Source Tokens | [svg](results/charts/20260628T204547Z/t2-source-tokens.svg) | [png](results/charts/20260628T204547Z/t2-source-tokens.png) |
| Tier 2 Source LOC | [svg](results/charts/20260628T204547Z/t2-source-loc.svg) | [png](results/charts/20260628T204547Z/t2-source-loc.png) |

## Tier 2 Runtime Plumbing

| framework | scheduler/rollup code | live delivery code | app-facing shape |
| --- | --- | --- | --- |
| vyuh | `#[bundles::periodic]` emits typed tick into `#[bundles::signal]` | `site.signals().emit`, `Subscriber`, `ChannelResponse` | runtime surfaces are registered and introspectable |
| axum | explicit `tokio::spawn` interval loop | explicit `broadcast::Sender`, SSE stream filter, long-poll loop | manual app state and plumbing |
| rocket | explicit spawned interval loop | explicit broadcast channel plus `EventStream` route | manual managed state and plumbing |
| actix | explicit spawned interval loop | explicit broadcast channel plus streaming response | manual app state and plumbing |
| fastapi | explicit `asyncio.create_task` rollup loop | explicit per-project queue set plus `sse-starlette` | manual queues and task lifecycle |
| drf | explicit background thread rollup loop | explicit event loop thread, queues, streaming response | folder-shaped Django app plus manual threads |


## Memory Snapshot

RSS is summed over the server process tree where available. Values are from this single run.

| target | idle RSS MiB | max after-scenario RSS MiB |
| --- | ---: | ---: |
| t1/vyuh | 11.5 | 22.8 |
| t1/axum | 8.7 | 18.8 |
| t1/rocket | 9.7 | 21.2 |
| t1/actix | 12.1 | 20.8 |
| t1/fastapi | 57.5 | 400.7 |
| t1/drf | 84.5 | 186.5 |
| t2/vyuh | 12.3 | 49.2 |
| t2/axum | 10.0 | 19.6 |
| t2/rocket | 11.2 | 23.2 |
| t2/actix | 14.2 | 22.0 |
| t2/fastapi | 60.3 | 67.5 |
| t2/drf | 84.2 | 97.5 |

## Rust Binary Size

| binary | size MiB |
| --- | ---: |
| t1_vyuh | 39.8 |
| t1_axum | 6.7 |
| t1_rocket | 7.1 |
| t1_actix | 8.3 |
| t2_vyuh | 39.7 |
| t2_axum | 5.7 |
| t2_rocket | 6.1 |
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

- Vyuh Tier 1 is in the same broad performance band as Axum for typed handlers. At c64, Vyuh echo is `8799 rps` versus Axum `8855 rps`; Vyuh item lookup is `8611 rps` versus Axum `8491 rps`.
- Python stacks are materially lower in this run. At Tier 2 c256 writes, Vyuh reports `9215 rps`; DRF reports `334 rps`.
- All measured scenario rows completed with the recorded HTTP-status error counts shown above.
- Tier 2 implementation ergonomics remain the more important Vyuh strength: scheduled rollup, signal fanout, channel publish, SSE, long-poll, and operation metadata are registered runtime surfaces instead of separately assembled background/pubsub machinery.

Raw data: `results/raw/20260628T204547Z/`.


## Limitations

- This is not yet a five-run median with min/max.
- The standard-library runner is intentionally dependency-free but is not a replacement for `oha`, `wrk`, or `bombardier`.
- The p99 values in Tier 2 contain repeated multi-second outliers across frameworks, so they are recorded but not used for ranking.
- SSE/long-poll correctness routes exist in the apps, but this run measured HTTP read/write endpoints only; live subscriber fanout still needs a dedicated scenario.
- Startup readiness is not reported for this raw run because the first harness version recorded it after readiness completed; the harness is fixed for future runs.
- Python apps ran single worker/process baselines, matching the single-process Rust baseline.

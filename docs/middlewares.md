# Middlewares

Vyuh separates global HTTP transport policy from feature-level route
composition. Site-wide middleware is configured with `SiteConf::http(...)`.
Bundle and route middleware remain available for feature-specific behavior.

## Overview

The main public pieces are:

- `SiteConf::http(HttpConf)` for global middleware configuration.
- `SlashPolicy` for deterministic trailing-slash behavior.
- `Bundle::with_slash_policy(...)` for bundle-level slash policy.
- `RouteConf { slash: Some(...), .. }` and `#[bundles::route(..., slash = "...")]`
  for route-level slash policy.
- `routes::Middleware` and `routes::layer_from(...)` for route or bundle
  middleware.

Site-wide middleware is applied through the shared internal router path used by
`Site::serve`, `site.start()`, and test router construction.

## Site HTTP Configuration

Start from defaults and enable only the transport behavior the application
needs:

```rust
use vyuh::{
    SiteConf,
    middlewares::{HttpConf, TraceConf, CompressionConf, BodyLimitConf},
};

let conf = SiteConf::default().http(HttpConf {
    trace: TraceConf { enabled: true },
    compression: CompressionConf { enabled: true },
    body_limit: BodyLimitConf {
        enabled: true,
        max_bytes: 1024 * 1024,
    },
    ..HttpConf::default()
});
```

Default behavior:

| Option | Default |
| --- | --- |
| panic catching | enabled |
| request id | enabled, `x-request-id` |
| slash policy | `Auto` |
| trace | disabled |
| compression | disabled |
| CORS | disabled |
| timeout | disabled |
| body limit | disabled |
| security headers | disabled |

## Request Ids And Panics

Request IDs are enabled by default. Vyuh reads the configured header when it is
present, otherwise it generates a new ID and writes it to the response:

```rust
use vyuh::middlewares::{HttpConf, RequestIdConf};

let conf = vyuh::SiteConf::default().http(HttpConf {
    request_id: RequestIdConf {
        enabled: true,
        header: "x-request-id".into(),
    },
    ..HttpConf::default()
});
```

Panic catching is also enabled by default so panics are converted into framework
errors instead of tearing down the server task.

## Trace, Compression, CORS, Timeout, And Limits

Trace, compression, CORS, timeout, and body limit are opt-in:

```rust
use vyuh::middlewares::{CorsConf, HttpConf, TimeoutConf};

let conf = vyuh::SiteConf::default().http(HttpConf {
    cors: CorsConf {
        enabled: true,
        permissive: true,
    },
    timeout: TimeoutConf {
        enabled: true,
        timeout_ms: 10_000,
    },
    ..HttpConf::default()
});
```

Timeout and body-limit failures flow through `ErrorReport` and the site error
handler, so custom API or HTML error pages can render them consistently.

## Security Headers

Security headers are disabled by default because applications often need
deployment-specific policy. Enable the built-in defaults when they fit:

```rust
use vyuh::middlewares::{HttpConf, SecurityHeadersConf};

let conf = vyuh::SiteConf::default().http(HttpConf {
    security_headers: SecurityHeadersConf {
        enabled: true,
        ..SecurityHeadersConf::default()
    },
    ..HttpConf::default()
});
```

The default header policy includes `x-content-type-options: nosniff`,
`x-frame-options: DENY`, and `referrer-policy: same-origin`.

## Slash Policy

Vyuh does not silently hard-code one trailing-slash rule for the whole server.
Slash behavior is route metadata:

| Policy | Behavior |
| --- | --- |
| `Exact` | only the declared path matches |
| `Trim` | alternate trailing slash rewrites internally |
| `RedirectAppend` | missing slash redirects to slash form with `308` |
| `RedirectRemove` | trailing slash redirects to non-slash form with `308` |
| `Auto` | HTML routes redirect to the declared path shape; API/unknown routes trim |

Site default:

```rust
use vyuh::middlewares::{HttpConf, SlashConf, SlashPolicy};

let conf = vyuh::SiteConf::default().http(HttpConf {
    slash: SlashConf {
        policy: SlashPolicy::Auto,
    },
    ..HttpConf::default()
});
```

Bundle override:

```rust
use vyuh::middlewares::SlashPolicy;

let bundle = app_bundle().with_slash_policy(SlashPolicy::RedirectAppend);
```

Route override with the macro:

```rust
#[vyuh::bundles::route(path = "/docs/", slash = "redirect_append")]
async fn docs() -> vyuh::routes::Html<&'static str> {
    vyuh::routes::Html("docs")
}
```

Route override with direct registration:

```rust
use std::borrow::Cow;
use vyuh::{
    bundles,
    middlewares::SlashPolicy,
    routes::{Methods, RouteConf},
};

let route = bundles::route(
    docs,
    RouteConf {
        name: Cow::Borrowed("docs"),
        path: Cow::Borrowed("/docs/"),
        methods: Methods::GET,
        slash: Some(SlashPolicy::RedirectAppend),
    },
);
```

Slash aliases and redirects are validated at site build. Conflicting generated
rules fail build instead of producing ambiguous runtime behavior.

## API And HTML Defaults

`Auto` is designed for mixed applications:

- API and unknown-response routes trim, so `/api/items/` can serve `/api/items`.
- HTML routes canonicalize to the declared path shape. A declared `/docs/`
  redirects `/docs` to `/docs/`; a declared `/about` redirects `/about/` to
  `/about`.

HTML detection uses route return metadata with `text/html`. Vyuh does not infer
slash policy from request `Accept` headers.

## Route And Bundle Middleware

Use site-wide middleware for global transport policy. Use bundle or route
middleware for feature-specific behavior and OpenAPI metadata:

```rust
let bundle = vyuh::bundles::bundle! {
    // routes
}
.with_middleware(vyuh::routes::layer_from(my_tower_layer));
```

Direct Tower or Axum layers remain escape hatches for behavior Vyuh does not
wrap yet. Prefer Vyuh's config and wrapper APIs when they cover the use case so
errors, OpenAPI metadata, and future compatibility remain consistent.

## Examples

- [`middlewares_global.rs`](../vyuh/examples/middlewares/global.rs): site-wide
  HTTP middleware configuration.
- [`middlewares_path_normalization.rs`](../vyuh/examples/middlewares/path_normalization.rs):
  slash policy behavior.

## Failure Modes

- Invalid slash policies or generated slash aliases fail during site build.
- Timeout and body-limit failures are rendered through the normal error
  pipeline.
- Panics are converted to framework errors when panic catching is enabled.

## Current Limitations

- Built-in middleware configuration covers common transport policy, not every
  Tower layer.
- Direct Tower layers remain available, but they do not automatically provide
  Vyuh OpenAPI metadata.
- Slash policy is based on route metadata, not request `Accept` headers.

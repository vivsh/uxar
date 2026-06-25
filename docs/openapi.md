# OpenAPI

OpenAPI is generated from Vyuh route metadata. Route configuration, handler
signatures, doc comments, middleware metadata, and explicit patches are combined
into an OpenAPI 3 spec at site build time.

OpenAPI is a first-class subsystem. It is commonly used with routes, but it has
its own configuration, schema conversion, response metadata, and override APIs.

## Overview

OpenAPI generation uses these inputs:

- `RouteConf` supplies the path, route name, and HTTP methods.
- Handler arguments supply path, query, body, and ignored state metadata.
- Handler return types supply response body, content type, and default status
  metadata.
- Doc comments supply operation summary and description.
- `PatchOp` overrides names, descriptions, argument metadata, response metadata,
  status codes, and extra responses.
- `OpenApiConf` controls the generated spec endpoint and API metadata.

## Registration

OpenAPI is registered on a `Bundle` with `with_openapi`. By default, Vyuh
serves only the JSON spec:

```rust
let bundle = routes.with_openapi(
    bundles::OpenApiConf::default()
        .title("Notes API")
        .version("0.1.0"),
);
```

Use `.spec(...)` to place the JSON spec under the same prefix as the API it
describes:

```rust
let bundle = routes.with_openapi(
    bundles::OpenApiConf::default()
        .title("Notes API")
        .version("0.1.0")
        .description("Notes service API")
        .spec("/api/openapi.json"),
);
```

Add `.viewer(...)` when the site should also serve an HTML documentation UI.
Swagger UI is the default viewer:

```rust
let bundle = routes.with_openapi(
    bundles::OpenApiConf::default()
        .spec("/api/openapi.json")
        .viewer("/api/docs"),
);
```

Use `.viewer_with(...)` to choose another built-in viewer:

```rust
let bundle = routes.with_openapi(
    bundles::OpenApiConf::default()
        .spec("/api/openapi.json")
        .viewer_with("/api/docs", bundles::DocViewer::Redoc),
);
```

The JSON spec is generated during site build. Schema conversion or serialization
errors fail startup instead of failing on the first documentation request.

## Order Sensitivity

`with_openapi` snapshots the route operations that are already registered in the
bundle. Routes added or merged after `with_openapi` will not appear in that
generated spec.

Register OpenAPI after all route registration and bundle merge steps for the API
surface the spec should describe. Prefixes and metadata applied to already
captured routes still affect the final generated paths and operation metadata.

See [Bundles](bundles.md) for bundle composition rules and the general
order-sensitive behavior of bundle-level APIs.

## Schemas

Request and response schemas come from `JsonSchema` types used in extractors and
returns:

```rust
use vyuh::prelude::*;

#[derive(Deserialize, JsonSchema)]
struct CreateNote {
    title: String,
}

#[derive(Serialize, JsonSchema)]
struct Note {
    id: i64,
    title: String,
}
```

`Json<CreateNote>` is emitted as an `application/json` request body.
`Json<Note>` is emitted as an `application/json` response body. Shared schemas
are emitted into OpenAPI components when schemars produces reusable definitions.

## Validation Metadata

Validation metadata is opt-in at the route boundary. Deriving `Validate` on a
type does not automatically add validation constraints to every route that uses
that type.

Plain wrappers document parse shape only:

```rust
async fn create(Json(input): Json<CreateNote>) {
    // OpenAPI uses the plain JsonSchema for CreateNote.
}
```

`Valid<E>` documents supported validation constraints and runs runtime
validation:

```rust
#[derive(Deserialize, JsonSchema, Validate)]
struct CreateNote {
    #[validate(min_length = 3)]
    title: String,
}

async fn create(Valid(Json(input)): Valid<Json<CreateNote>>) {
    // OpenAPI includes minLength for title, and runtime validation returns 422.
}
```

Vyuh emits only constraints that can be represented accurately in OpenAPI, such
as string length, numeric ranges, formats, patterns, collection sizes, enum
values, and explicit custom validator hints. Runtime-only validators such as
`custom` remain enforcement logic only unless they opt in with
`custom_schema = "name"`, which emits `x-vyuh-validators` vendor metadata for
clients.

See [Validation](validation.md) for the full validation model.

## Response Metadata

Vyuh infers the primary response from the handler return type. `PatchOp` can
override the inferred response status and description through the direct API:

```rust
bundles::route(create_note, conf).patch(
    PatchOp::new()
        .ret()
        .status(201)
        .doc("Created note")
        .done(),
)
```

The same response override can be written on the route macro:

```rust
#[bundles::route(
    path = "/notes",
    method = "POST",
    returns(status = 201, description = "Created note")
)]
async fn create_note(Json(input): Json<CreateNote>) -> Json<Note> {
    Json(Note {
        id: 1,
        title: input.title,
    })
}
```

Additional responses are appended with `PatchOp::append()`:

```rust
PatchOp::new()
    .append()
    .status(409)
    .typed::<Json<ApiError>>()
    .doc("Title already exists")
    .done()
```

Equivalent macro syntax uses `returns(ty = "...")` for appended response
metadata:

```rust
#[bundles::route(
    path = "/notes",
    method = "POST",
    returns(status = 201, description = "Created note"),
    returns(ty = "Json<ApiError>", status = 409, description = "Title already exists")
)]
async fn create_note(Json(input): Json<CreateNote>) -> Json<Note> {
    Json(Note {
        id: 1,
        title: input.title,
    })
}
```

This is useful for documented error responses, alternate success responses, and
handlers returning raw `Response`.

## Argument Overrides

Argument names and descriptions are usually extracted from the handler. `PatchOp`
can adjust argument metadata by position through the direct API:

```rust
PatchOp::new()
    .arg(0)
    .name("id")
    .doc("Note id")
    .done()
```

The same override can be written on the route macro:

```rust
#[bundles::route(
    path = "/notes/{id}",
    arg(pos = 0, name = "id", ty = "i64", description = "Note id")
)]
async fn get_note(Path(id): Path<i64>) -> Json<Note> {
    Json(Note {
        id,
        title: "example".to_string(),
    })
}
```

The patch applies only to metadata. Runtime extraction still follows the handler
signature.

## Middleware Metadata

Middleware that implements `routes::Middleware` can return a `LayerSpec`. Layer
parts contribute OpenAPI parameters to every operation in the layered bundle.
`routes::layer_from(layer)` applies a Tower layer without OpenAPI metadata.

## Examples

Run the OpenAPI examples in increasing complexity:

```sh
cargo run -p vyuh --no-default-features --features sqlite --example openapi_basic
cargo run -p vyuh --no-default-features --features sqlite --example openapi_responses
```

- `openapi_basic`: spec registration for a route bundle.
- `openapi_responses`: macro and `PatchOp` response overrides, documented error
  responses, and a custom error schema.

## Failure Modes

OpenAPI failures are reported during site build:

- Unsupported schema conversion.
- JSON serialization failure for the generated spec.
- Hidden OpenAPI routes colliding with existing route paths.

`CONNECT` routes can be served by Vyuh but are not represented as OpenAPI
operations because OpenAPI 3 does not model `CONNECT`.

## Best Practices

- Keep handler doc comments user-facing.
- Use `PatchOp` for non-200 success statuses and documented error responses.
- Prefer concrete request and response structs that derive `JsonSchema`.
- Keep spec endpoints under the same prefix as the API they describe.

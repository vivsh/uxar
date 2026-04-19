# Routing

## Purpose

- Maps HTTP methods and URL patterns to async handler functions.
- Provide composition and registration points for route trees.
- Enable OpenAPI metadata collection and reverse routing.

## API Surface

- Name: `route`
  Kind: `macro` (attribute)
  Signature: `#[route(method = "<method>", url = "<path>", ...)]`
  Inputs: macro attributes (`method`, `url`, optional `tags`, `name`, `summary`, `description`) and a function or method item.
  Output: registers handler into a `Bundle` when used inside bundle macros; compiles to an `Operation` at registration time.
  Errors: compile-time macro errors for invalid attributes; runtime routing conflicts reported by `Site` during registration.
  Side Effects: none at runtime beyond normal handler registration.

- Name: `RouteConf`
  Kind: `struct` (re-export)
  Signature: `pub struct RouteConf { /* platform-specific fields */ }`
  Inputs: configuration values passed to site/build.
  Output: influences route mounting and OpenAPI behavior.
  Errors: `None` (config validated elsewhere).
  Side Effects: none.

- Name: `AxumRouter` (re-export)
  Kind: `type`/`module` (re-export of axum Router)
  Signature: `AxumRouter` (see axum::Router)
  Inputs: route handlers and method filters.
  Output: composed router.
  Errors: runtime HTTP handling errors.
  Side Effects: none.

- Name: `Operation`
  Kind: `struct`
  Signature: opaque internal type representing handler metadata
  Inputs: handler function, extractor specs, attributes
  Output: used by Bundle to build routing and docs
  Errors: None
  Side Effects: None

## Usage Examples

### Example 1

Goal: Define a GET handler that returns JSON.

```rust
use uxar::route;
use axum::extract::Path;
use uxar::Json;

#[route(method = "get", url = "/users/{id}")]
async fn get_user(Path(id): Path<i32>) -> Json<User> {
    Json(User { id })
}
```

Why valid:

- Handler signature matches expected extractors (`Path<i32>`).
- `method` and `url` are present as required macro attributes.

### Example 2

Goal: Named route and OpenAPI tags.

```rust
#[route(method = "post", url = "/users", name = "create_user", tags = ["users", "api"])]
async fn create_user(Json(payload): Json<CreateUser>) -> Json<User> {
    // ...
}
```

Why valid:

- Tag list attaches to OpenAPI metadata.
- `name` enables reverse routing and unique identification.

## Behavior Rules

- MUST require `method` and `url` attributes on `#[route]`.
- MUST accept `route` on free functions and impl methods.
- MUST allow optional `tags`, `name`, `summary`, `description` attributes.
- MUST register route metadata into the enclosing `Bundle` at compile/registration time.
- MUST cause compile-time error for malformed attribute values.
- MUST produce runtime registration error when duplicate route names or conflicting mounts occur.
- MUST treat route `tags` as additive when applied via `bundle!` tags.
- MUST support path parameters in `{name}` braces and validate extractor types at compile or runtime where possible.
- SHOULD produce OpenAPI operation entries when `openapi` features are enabled.
- MUST NOT perform side effects beyond registering metadata.

## Integration Guide

1. Import `route` macro via `use uxar::route` or `use uxar::*`.
2. Annotate handler functions or impl methods with `#[route(...)]` including `method` and `url`.
3. Group handlers into `bundle! { ... }` for composition and tag propagation.
4. Add bundles to `Site` during bootstrap: `site.register(bundle)`.
5. Ensure any extractors used (e.g., `Json<T>`, `Path<T>`) are in scope and their types implement required traits.
6. Enable `openapi` features and provide `OpenApiConf` to `Bundle` if API docs needed.

## Failure Modes

| Condition                                                                  | Observed Outcome                                                      | Fix                                                    |
| -------------------------------------------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------ |
| Missing `method` or `url` attribute                                        | Compile-time macro error                                              | Add required attributes to `#[route]`.                 |
| Duplicate `name` across routes                                             | `Site` registration error indicating duplicate route name             | Rename route or remove conflicting registration.       |
| Path parameter type mismatch (e.g., `{id}` but extractor expects `String`) | Runtime extractor parse error or handler rejection                    | Use matching extractor type or change path param type. |
| Malformed attribute (invalid method)                                       | Compile-time macro error                                              | Use valid HTTP method string.                          |
| Route conflict on same path/method mount                                   | Registration failure or last-writer behavior depending on mount order | Resolve mount ordering or consolidate handlers.        |

## Non-Goals

- Not a runtime router replacement; relies on axum for routing behavior.
- Not responsible for authentication or authorization checks (use `auth` features).
- Not responsible for database access patterns; those belong to `database` docs.

## LLM Recipe

1. Identify handler intent: HTTP method, URL pattern, input types, and output type.
2. Ensure required attributes: include `method` and `url` in `#[route]`.
3. Pick extractors matching path/query/body types (`Path<T>`, `Query<T>`, `Json<T>`).
4. Generate minimal handler signature returning `Json<T>` or `IntoResponse`.
5. Add `name` attribute when reverse routing required.
6. Add `tags` when endpoint should appear under OpenAPI groups.
7. Validate that path parameters in `url` align with `Path<T>` extractor types.
8. Keep handler body minimal; call into services or callables for business logic.
9. Run a compile check for macro attribute correctness.
10. Ensure examples include `use` imports for extractor types.
11. Anti-pattern: embedding heavy business logic directly in handler. Prefer delegating to `services` or `callables`.
12. Final check: doc metadata (`summary`/`description`) present if API documentation is expected.

# Bundle Composition

## Purpose

- Groups routes, tasks, signals, emitters, services, assets, and docs into one registration unit.
- Provides composition by merge, nest, and free helper constructors.
- Accumulates registration errors before site build validates the bundle.

## API Surface

- Name: `Bundle`
  Kind: `struct`
  Signature: `pub struct Bundle { ... }`
  Inputs: created by `Bundle::new()` or `Bundle::from_parts(...)`.
  Output: immutable composition unit for site bootstrap.
  Errors: `BundleError` via `validate()`.
  Side Effects: stores routes, registries, asset dirs, and doc config.

- Name: `BundleError`
  Kind: `enum`
  Signature: `pub enum BundleError { Signal, Task, Emitter, Service, Command, ErrorList }`
  Inputs: merge, registration, and constructor failures.
  Output: accumulated bundle validation error.
  Errors: `None`.
  Side Effects: `None`.

- Name: `IntoBundle`
  Kind: `trait`
  Signature: `pub trait IntoBundle { fn into_bundle(self) -> Bundle; }`
  Inputs: implemented for `Bundle` and `axum::Router<Site>`.
  Output: normalized `Bundle`.
  Errors: `None`.
  Side Effects: `None`.

- Name: `OpenApiConf`
  Kind: `struct`
  Signature: `pub struct OpenApiConf { pub doc_path: String, pub spec_path: String, pub meta: ApiMeta, pub viewer: DocViewer }`
  Inputs: doc path, spec path, API metadata, viewer type.
  Output: OpenAPI doc/spec bundle part.
  Errors: OpenAPI generation failures surface later through served output or bundle use.
  Side Effects: installs doc and spec routes when injected.

- Name: `Bundle` methods
  Kind: `fn`
  Signature: `new`, `id`, `label`, `validate`, `to_router`, `iter_routes`, `iter_operations`, `iter_views`, `reverse`, `with_router_unchecked`, `from_parts`
  Inputs: receiver `&self` or `self` plus method-specific args.
  Output: router views, metadata iterators, reverse paths, or validated bundle.
  Errors: `BundleError` for `validate`; `None` otherwise.
  Side Effects: `with_router_unchecked` replaces the current router.

- Name: `BundlePart`
  Kind: `struct`
  Signature: `pub struct BundlePart { ... }`
  Inputs: returned from bundle helper constructors.
  Output: single composable bundle item.
  Errors: embedded errors can be stored inside the part.
  Side Effects: `patch` mutates stored operation metadata before collection.

- Name: bundle helper functions
  Kind: `fn`
  Signature: `route`, `cron`, `service`, `periodic`, `pgnotify`, `signal`, `command`, `merge`, `nest`, `openapi`, `tags`, `asset_dir`, `bundle`
  Inputs: handler functions, config structs, sub-bundles, tags, or embedded dirs.
  Output: `BundlePart` or `Bundle`.
  Errors: helper-specific registration errors are stored as `BundleError` inside the bundle.
  Side Effects: attach routes, registries, docs, tags, or assets to the resulting bundle.

- Name: `bundle!`
  Kind: `macro`
  Signature: `bundle! { item1, item2, ... }`
  Inputs: macro-generated bundle parts from route/task/signal/emitter/service declarations.
  Output: `Bundle`.
  Errors: macro expansion and later bundle validation errors.
  Side Effects: `None` beyond bundle construction.

## Usage Examples

### Example 1

Goal: Create an empty bundle.

```rust
use uxar::bundles::Bundle;

let bundle = Bundle::new();
assert!(bundle.validate().is_ok());
```

Why valid:

- `Bundle::new()` creates an empty bundle.
- Validation succeeds when no registration errors were accumulated.

### Example 2

Goal: Merge two bundles into one.

```rust
use uxar::bundles::{Bundle, bundle, merge};

let left = Bundle::new();
let right = Bundle::new();
let merged = bundle([merge(left), merge(right)]);
```

Why valid:

- `merge` accepts any `IntoBundle`.
- Final collection happens through `bundle([...])`.

### Example 3

Goal: Mount a sub-bundle below a path prefix.

```rust
use uxar::bundles::{Bundle, bundle, nest};

let api = Bundle::new();
let root = bundle([nest("/api", "api", api)]);
```

Why valid:

- `nest` records both a path prefix and an operation namespace.
- Nested bundles stay composable because `nest` returns a `BundlePart`.

## Behavior Rules

- MUST collect non-route parts and route metadata into one `Bundle` value.
- MUST store registration errors instead of panicking during bundle construction.
- MUST return `BundleError::ErrorList` from `validate()` when errors were accumulated.
- MUST accept `Bundle` and `axum::Router<Site>` through `IntoBundle`.
- MUST merge routes through Axum router merge semantics.
- MUST merge signals, emitters, tasks, services, and commands through their registries.
- MUST namespace nested route metadata through `nest(path, namespace, ...)`.
- MUST require nested mount paths to start with `/` in debug builds.
- MUST reject duplicate emitter, task, service, or command registrations through stored bundle errors.
- MUST allow reverse routing only for named route operations present in `meta_map`.
- MUST treat `with_router_unchecked` as an overwrite of the current router.
- MUST treat `bundle!` as sugar over explicit bundle part construction.
- `tags(...)` exists, but current code does not apply tags to operations yet.

## Integration Guide

1. Start with `Bundle::new()` or `bundle([...])`.
2. Add route and non-route parts through the helper functions or macro-generated parts.
3. Use `merge` to combine peer bundles.
4. Use `nest` to mount a sub-bundle below a path prefix and namespace.
5. Call `validate()` if code wants early failure before site build.
6. Pass the final bundle into `build_site` or `serve_site`.
7. Use `reverse` or `iter_operations` when docs or runtime code need route metadata.

## Failure Modes

| Condition                                                 | Observed Outcome                                  | Fix                                                              |
| --------------------------------------------------------- | ------------------------------------------------- | ---------------------------------------------------------------- |
| Duplicate emitter, task, service, or command registration | Stored `BundleError` and `validate()` fails later | Rename or deduplicate the conflicting part.                      |
| Bundle contains any stored errors                         | `BundleError::ErrorList` from `validate()`        | Inspect the stored registration cause and rebuild the bundle.    |
| Invalid nested mount path in debug builds                 | Debug assertion failure                           | Use a mount path that starts with `/` and does not end with `/`. |
| Reverse lookup is called for an unknown route name        | `None` is returned                                | Use the registered route name or add the route to the bundle.    |
| Invalid OpenAPI generation                                | Spec route serves an error JSON payload           | Fix the underlying route or schema metadata.                     |

## Non-Goals

- Does not start the site or the HTTP server.
- Does not replace route, task, signal, or logging feature docs.
- Does not guarantee conflict-free route semantics beyond Axum router behavior.

## LLM Recipe

1. Choose `Bundle` as the unit of feature registration.
2. Build bundle parts explicitly with helper functions when code generation needs full control.
3. Use `bundle!` only as syntax sugar when macro-generated parts already exist.
4. Use `merge` for peer composition and `nest` for path-scoped composition.
5. Call `validate()` before handing the bundle to site bootstrap in generated tests.
6. Generate named routes when reverse lookup will be used later.
7. Treat `with_router_unchecked` as advanced and avoid it unless extra router layering is required.
8. Anti-pattern: assuming `tags(...)` currently mutates route metadata.
9. Anti-pattern: generating duplicate task, emitter, service, or command names across merged bundles.
10. Final check: ensure the output passed to site bootstrap implements `IntoBundle`.

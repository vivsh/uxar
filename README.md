# Uxar

A strongly opinionated Rust web framework built on Axum, designed for rapid development of Postgres-backed JSON APIs with JWT authentication.

## Philosophy

Uxar embraces convention over configuration while maintaining Rust's safety guarantees. 

The framework follows a "site as tapestry" model where all components—routing, authentication, database, templates—are woven together through a central `Site` builder pattern.

## Status

**Alpha/Experimental** - Under active development. APIs will change. Not recommended for production use.

Project status note:
- Uxar is built primarily for my personal projects, shaped by the constraints and preferences of that work.
- It’s published so others can benefit from the ideas, patterns, and building blocks.
- Expect sharp edges and occasional breaking changes.

Current state:
- Core routing and site scaffolding: Functional
- JWT authentication: Functional with cookie and header support
- Database layer: Basic query building and migrations framework in place
- Validation: Comprehensive derive macro with OpenAPI integration
- Documentation: Sparse; code is the primary documentation

## Features

### Site Scaffolding

The `Site` acts as the central orchestrator, providing a builder pattern to compose applications:

```rust
Site::builder(conf)
    .with_service(my_service)
    .mount("/api", "api_v1", api_router)
    .merge(UserView::as_routable())
    .run()
    .await?
```

Key capabilities:
- **Service container**: Type-safe dependency injection via `with_service` and `get_service`
- **Template rendering**: Integrated MiniJinja environment loaded from embedded or filesystem templates
- **Static file serving**: Configurable static directories with automatic `ServeDir` mounting
- **Database pooling**: Managed Postgres connection pool with configurable limits
- **Reverse routing**: Name-based URL generation via `reverse`
- **Configuration**: Environment-based config loading with `.env` support (see `SiteConf`)

### Authentication

JWT-based authentication with support for both access and refresh tokens:

```rust
let user = AuthUser::extract_from_request_parts(parts, &site)?;
```

Features:
- Dual token system (access + refresh) with configurable TTLs
- Cookie-based and header-based token extraction
- Audience (`aud`) claim validation for multi-tenant scenarios
- Automatic cookie management with `HttpOnly`, `Secure`, and `SameSite` controls
- Custom authentication backend trait (`AuthBackend`) for pluggable auth strategies
- Role-based access control with compile-time bitmask permissions

### Database

Type-safe query builder with schema-driven development:

```rust
#[derive(Model, Debug)]
#[model(db_table = "users")]
struct User {
    #[field(primary_key)]
    id: i32,
    
    #[field(unique, db_indexed)]
    email: String,
    
    #[field(db_check = "age >= 18")]
    age: i32,
    
    bio: Option<String>,  // Automatically detected as nullable
}

// Type-safe query building
let users: Vec<User> = User::query()
    .select()
    .filter("is_active = ?")
    .bind(true)
    .all(&mut tx)
    .await?;
```

Key components:
- **Model derive macro**: Generates schema metadata with validation integration
  - Field-level attributes: `primary_key`, `unique`, `unique_group`, `db_column`
  - Database constraints: `db_indexed`, `db_index_type`, `db_default`, `db_check`
  - Query control: `selectable`, `insertable`, `updatable` for fine-grained access
  - Automatic nullable detection from `Option<T>` types
- **Scannable/Bindable**: Type-safe row scanning and parameter binding
- **Query builder**: Fluent API with `select()`, `insert()`, `update()`, `filter()`, `order_by()`, `slice()`
- **Filterable**: Trait for composable WHERE clauses
- **JSON aggregation**: Built-in `fetch_json_*` methods for Postgres `JSONB_AGG` queries

Schema metadata in `ColumnSpec` includes:
- Column type and nullability
- Validation rules (integrated with validation framework)
- Database constraints (primary keys, unique constraints, indexes, checks)
- Query visibility (selectable/insertable/updatable flags)

The `Model` trait combines `SchemaInfo`, `Scannable`, and `Bindable` for comprehensive type-safe database operations.


### Routing

Type-safe, name-based routing with automatic metadata extraction:

```rust
#[routable]
impl UserView {
    #[route(method = "GET", url = "/users/{id}")]
    async fn get_user(Path(id): Path<i32>) -> Json<User> {
        // handler
    }
}
```

Features:
- **Named routes**: Use `site.reverse("get_user", &[("id", "42")])` for URL generation
- **Nested mounting**: `mount` and `merge` for composing routers with namespaces
- **Automatic metadata**: `ViewMeta` extracted from handlers for docs/OpenAPI generation
- **Parameter introspection**: Automatically detects Axum extractors (`Path`, `Query`, `Json`) and generates `ParamMeta`
- **Response schemas**: Multi-status response metadata with optional type information (`ReturnMeta`)
- **Base path support**: `#[routable(base_path = "/api/v1")]` for grouped endpoints

The `#[routable]` macro generates a `StaticRoutable` implementation returning both the Axum router and view metadata, enabling documentation generation and reverse routing.

### Validation

Comprehensive validation framework with derive macro:

```rust
#[derive(Validatable)]
struct User {
    #[validate(email)]
    email: String,
    
    #[validate(min_length = 3, max_length = 50)]
    username: String,
    
    #[validate(range = (18, 120))]
    age: i32,
}

// Use with Axum extractors
async fn handler(Valid(Json(user)): Valid<Json<User>>) {
    // user is validated
}
```

Supported validators:
- `email`, `url`, `uuid`, `ipv4`
- `min_length`, `max_length`, `exact_length`
- `min_value`, `max_value`, `range`
- `regex`, `alphanumeric`, `slug`, `digits`
- `non_empty`

Validation metadata is integrated into `ColumnSpec` via the `Model` macro for database schema generation and OpenAPI documentation.

The `Validatable` trait can be used standalone for types that don't interact with the database, while `Model` includes validation automatically.

### Tasks

Uxar includes a small background task engine intended for app-internal work (scheduled jobs, async maintenance, best-effort side effects). This is not a distributed queue.

Typical usage is to wire the task engine into your `Site` and have application code enqueue or trigger tasks through it.

### Beacon

`Beacon` is Uxar’s lightweight server→client publish/subscribe primitive intended for SSE.

Key properties:
- Subscribers are tracked by `AuthUser`.
- Publishing supports `Target::User`, `Target::RoleMask`, or `Target::All`.
- Each subscriber has a bounded queue; if it’s full, the newest message is dropped (best-effort delivery).
- Optional exclusivity: allow only one active subscription per user.

## Non-Goals

Uxar is **not** trying to be:

- **Framework-agnostic**: Tightly coupled to Axum, Postgres, and JWT; no abstraction over these choices
- **ORM replacement**: Not competing with Diesel or SeaORM; provides lightweight query building only
- **Microservices-first**: Designed for monolithic APIs; no built-in service mesh or distributed tracing
- **Frontend framework**: No opinions on React/Vue/etc.; JSON API focus
- **GraphQL/gRPC**: REST/JSON only; no alternative protocol support
- **Multi-database**: Postgres-only; no plans for MySQL, SQLite, etc.
- **Backward compatible**: Will break APIs frequently during alpha/beta

## TODO

### High Priority
- [ ] OpenAPI spec generation from `ViewMeta` and `Schemable`
- [ ] CSRF protection for cookie-based auth
- [ ] Rate limiting middleware
- [ ] Comprehensive error types with proper context propagation
- [ ] Improved tracing logs and better module organisation

### Medium Priority
- [ ] CLI tool for scaffolding projects/apps
- [ ] Email sending abstraction
- [ ] Background task queue (stubbed)
- [ ] WebSocket support with authentication

### Low Priority
- [ ] Health check endpoints
- [ ] Metrics/observability integration

### Documentation
- [ ] Comprehensive README with examples
- [ ] API documentation for all public items
- [ ] Migration guide for upgrading between versions
- [ ] Tutorial: building a blog API
- [ ] Comparison with Actix-web, Rocket, Loco

## License

MIT
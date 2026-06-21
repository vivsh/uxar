# Request Data

Vyuh request data wrappers are the normal way to receive route input. They keep
Axum extractor details behind Vyuh's API boundary and make parsing, errors, and
OpenAPI metadata consistent across routes.

Vyuh wrappers are intentionally thin. They delegate parsing behavior to Axum
internally, then convert failures into Vyuh's `ErrorReport` shape. Wrapping the
extractors gives Vyuh consistent errors, first-class OpenAPI integration, and a
stable public API even if the internal Axum integration changes.

All parse failures become `ErrorReport` values and are rendered through the
site's configured error handler.

## Mental Model

- Request wrappers parse incoming data.
- Request wrappers never validate.
- `Valid<E>` adds validation.
- Parse failures return `400`.
- Validation failures return `422`.

The route lifecycle is:

```text
Request -> Wrapper Parse -> Valid (optional) -> Handler -> Response
```

Use these from `vyuh::routes`:

| Wrapper | Source | Parse failure | OpenAPI behavior |
| --- | --- | --- | --- |
| `Json<T>` | JSON body | `400` | JSON request body or response body |
| `Query<T>` | query string | `400` | query parameters |
| `Path<T>` | path captures | `400` | path parameters |
| `Form<T>` | URL-encoded form body | `400` | form request body |
| `BodyBytes` | raw body bytes | `400` | opaque binary request body |

Wrappers parse only, even when the DTO derives `Validate`. Validation runs only
when a wrapper is wrapped in `Valid<E>`. See [Validation](validation.md).

## JSON

`Json<T>` parses an `application/json` request body:

```rust
use vyuh::routes::Json;

#[derive(serde::Deserialize)]
struct CreateNote {
    title: String,
}

async fn create(Json(input): Json<CreateNote>) {
    // input is parsed, but not validated.
}
```

`Json<T>` can also be returned from handlers when `T: Serialize`:

```rust
#[derive(serde::Serialize)]
struct NoteOut {
    id: u64,
}

async fn show() -> Json<NoteOut> {
    Json(NoteOut { id: 1 })
}
```

Invalid JSON or a body that cannot deserialize into `T` returns `400`.

## Query

`Query<T>` parses query strings:

```rust
use vyuh::routes::Query;

#[derive(serde::Deserialize)]
struct SearchParams {
    q: String,
    page: Option<u32>,
}

async fn search(Query(params): Query<SearchParams>) {
    // /search?q=vyuh&page=2
}
```

Malformed query strings or failed deserialization return `400`.

Unknown query parameters follow Serde deserialization behavior. They are
ignored for ordinary structs, and rejected when the target type opts into
`#[serde(deny_unknown_fields)]`.

## Path

`Path<T>` parses path captures. Use a struct when names matter:

```rust
use vyuh::routes::Path;

#[derive(serde::Deserialize)]
struct UserPath {
    id: uuid::Uuid,
}

#[vyuh::bundles::route(path = "/users/{id}")]
async fn user_detail(Path(path): Path<UserPath>) {
    let id = path.id;
}
```

Use a tuple for positional captures:

```rust
use vyuh::routes::Path;

#[vyuh::bundles::route(path = "/orgs/{org}/users/{id}")]
async fn user_in_org(Path((org, id)): Path<(String, u64)>) {
    // org and id are parsed from the path.
}
```

Path parse failures return `400`.

## Form

`Form<T>` parses `application/x-www-form-urlencoded` request bodies:

```rust
use vyuh::routes::Form;

#[derive(serde::Deserialize)]
struct LoginForm {
    email: String,
    password: String,
}

async fn login(Form(form): Form<LoginForm>) {
    // form.email, form.password
}
```

Form parse failures return `400`.

Keep `MultipartForm<T>` documented separately once it is implemented. File
upload validation and OpenAPI behavior are substantially different from
standard URL-encoded forms.

## Raw Body

`BodyBytes` reads the request body as bytes. It is useful for webhooks because
signature verification usually needs the exact raw payload:

```rust
use vyuh::routes::BodyBytes;

async fn webhook(BodyBytes(bytes): BodyBytes) {
    // Verify the provider signature against the raw bytes before decoding.
}
```

Use it for custom protocols, signed payloads, or cases where deserialization is
not appropriate. `BodyBytes` is intentionally excluded from the validation
system because it represents raw request data.

`BodyBytes` does not generate a JSON schema. In OpenAPI it is documented as an
opaque binary request body.

## Ownership Helpers

Wrappers support pattern matching, `Deref`, `AsRef`, and `into_inner()`:

```rust
async fn create(json: Json<CreateNote>) {
    let input = json.into_inner();
}
```

`Valid<E>` supports the same ownership pattern around the wrapped extractor.

## Validation

Use `Valid<E>` when parsed input should be validated:

```rust
use vyuh::routes::{Json, Valid};

async fn create(Valid(Json(input)): Valid<Json<CreateNote>>) {
    // input is parsed and validated.
}
```

Parse failures and validation failures are different:

- A parse failure means Vyuh could not read the request into the wrapper type.
- A validation failure means parsing succeeded, but `Validate` rejected the
  parsed value.

Plain wrappers never publish validation constraints to OpenAPI. Validation
metadata is published only when `Valid<E>` is used. The full model is described
in [Validation](validation.md).

## OpenAPI

Wrappers contribute request metadata:

- `Json<T>` becomes a JSON request body.
- `Query<T>` becomes query parameters.
- `Path<T>` becomes path parameters.
- `Form<T>` becomes a form request body.
- `BodyBytes` becomes an opaque binary request body.

Given a DTO that derives `Validate`:

```rust
#[derive(serde::Deserialize, schemars::JsonSchema, vyuh::Validate)]
struct CreateNote {
    #[validate(min_length = 3)]
    title: String,
}
```

`Json<CreateNote>` documents only the parse shape:

```yaml
requestBody:
  content:
    application/json:
      schema:
        $ref: '#/components/schemas/CreateNote'
```

`Valid<Json<CreateNote>>` documents the parse shape plus supported validation
constraints:

```yaml
requestBody:
  content:
    application/json:
      schema:
        type: object
        properties:
          title:
            type: string
            minLength: 3
```

See [Validation](validation.md) for supported constraints and runtime-only
rules that are intentionally not emitted.

## Axum Escape Hatch

Vyuh wrappers are the recommended API. Direct Axum extractors remain possible
through explicit Axum imports, or through `vyuh::routes::axum_extractors` when a
route needs behavior Vyuh does not wrap yet.


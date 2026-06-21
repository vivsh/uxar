# Auth

Vyuh auth provides JWT-backed request authentication, cookie or header token
extraction, ergonomic role guards with `permit!`, and OpenAPI security metadata
from handler signatures.

Auth is intentionally small. Vyuh does not provide a user database or account
workflow in v0. Applications decide how users are loaded and verified, then use
Vyuh auth to issue tokens and protect routes.

## Overview

The main public pieces are:

- `AuthConf` for token lifetimes and cookie settings.
- `Authenticator` for encoding, decoding, login, refresh, and logout helpers.
- `AuthUser` for the authenticated subject and role mask.
- `#[derive(BitRole)]` for typed role enums.
- `permit!(Role, ...)` for concise role-protected route extraction.
- OpenAPI bearer security metadata generated from `AuthUser` and `permit!`.

The site owns an `Authenticator`, built from `SiteConf.auth` and
`SiteConf.secret_key`.

## Configuration

Auth configuration lives on `SiteConf`:

```rust
use vyuh::{
    SiteConf,
    auth::{AuthConf, CookieConf},
};

let conf = SiteConf::default()
    .secret_key("replace-with-a-long-random-secret")
    .auth(AuthConf {
        access_ttl: 3600,
        refresh_ttl: 604800,
        access_cookie: Some(CookieConf {
            name: "access_token".into(),
            ..CookieConf::default()
        }),
        refresh_cookie: Some(CookieConf {
            name: "refresh_token".into(),
            same_site: "Strict".into(),
            ..CookieConf::default()
        }),
    });
```

`access_ttl` and `refresh_ttl` are seconds. Tokens are signed with HS256 using
`SiteConf.secret_key`.

Access and refresh cookies are optional. When configured, the authenticator can
read tokens from cookies and write login/logout cookies. Requests can also pass
an access token in the `Authorization` header:

```text
Authorization: Bearer <jwt>
```

The older `Authorization: JWT <jwt>` form is also accepted.

## Authenticator

Use `site.authenticator()` when a route needs to issue or refresh tokens:

```rust
use vyuh::{Site, auth::{AuthError, AuthUser, TokenPair}, routes::Json};

async fn login(site: Site) -> Result<Json<TokenPair>, AuthError> {
    let user = AuthUser::new("user-123", 0);
    let tokens = site.authenticator().create_token_pair(user, &["web"])?;
    Ok(Json(tokens))
}
```

The main methods are:

- `create_token_pair(user, aud)` - create access and refresh JWTs.
- `login_user(user, aud, response)` - create tokens and set configured cookies.
- `refresh(parts, aud)` - read a refresh token and create a new token pair.
- `logout(refresh, response)` - clear the configured access or refresh cookie.
- `encode` and `decode` - lower-level JWT helpers.

Audience checks are opt-in. If both the token and caller provide audiences, at
least one value must match.

## AuthUser

Routes can extract `AuthUser` directly:

```rust
use vyuh::{auth::AuthUser, routes::Json};

async fn me(user: AuthUser) -> Json<String> {
    Json(user.key.to_string())
}
```

`AuthUser` contains:

- `key`: the authenticated subject, stored as the JWT `sub`.
- `roles`: a `u64` role mask.

Extracting `AuthUser` requires a valid access token. It also contributes bearer
auth metadata to OpenAPI.

## Roles And Permit

Define roles with `BitRole`:

```rust
use vyuh::auth::{BitRole, AuthUser};

#[derive(BitRole)]
enum AppRole {
    Manager,
    Editor,
    Viewer,
}

let user = AuthUser::new("user-123", AppRole::Manager.to_role_type());
```

Protect routes with `permit!`. It is designed to keep authorization close to
the handler signature, so protected routes read like ordinary typed extractors:

```rust
use vyuh::{auth::permit, routes::Json};

async fn managers_only(_permit: permit!(AppRole, Manager)) -> Json<&'static str> {
    Json("ok")
}
```

The route above requires a valid JWT and the `Manager` role. No separate
middleware wiring or route-side permission boilerplate is needed.

Role expressions support:

- `permit!(AppRole, Manager)` - requires one role.
- `permit!(AppRole, Manager | Editor)` - requires any listed role.
- `permit!(AppRole, Manager & Editor)` - requires all listed roles.

The permit extractor returns `401` for missing/invalid tokens and `403` when the
token is valid but lacks the required role mask. It also contributes the same
role requirement to OpenAPI automatically.

## OpenAPI

Auth is reflected in generated OpenAPI specs from handler arguments:

- `AuthUser` adds a `bearerAuth` security requirement with no role scopes.
- `permit!(Role, ...)` adds `bearerAuth` with role scopes.
- `permit!(Role, A | B)` records an any-role requirement.
- `permit!(Role, A & B)` records an all-role requirement.

Vyuh also emits a `bearerAuth` security scheme as HTTP bearer JWT:

```yaml
components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
      bearerFormat: JWT
```

OpenAPI spec endpoints can be protected independently with
`OpenApiConf::auth(...)`, which receives the extracted `AuthUser`.

## Password Utilities

Vyuh includes small PBKDF2 password helpers:

- `make_password(password, salt, algorithm)`
- `check_password(password, encoded)`
- `unusable_password()`

The encoded hash format is compatible with Django PBKDF2 hashes, but this is a
convenience, not the center of the auth subsystem. Vyuh does not prescribe where
users, password hashes, sessions, or refresh-token records are stored.

## Examples

- [`auth_basic.rs`](../vyuh/examples/auth_basic.rs): issue a JWT token pair and
  protect a route with `AuthUser`.
- [`auth_roles_openapi.rs`](../vyuh/examples/auth_roles_openapi.rs): role masks,
  `permit!`, and generated OpenAPI bearer security metadata.

## Failure Modes

- Missing tokens return `AuthError::MissingToken` and HTTP `401`.
- Invalid tokens return `AuthError::InvalidToken` and HTTP `401`.
- Expired tokens return `AuthError::ExpiredToken` and HTTP `401`.
- Invalid signatures return `AuthError::InvalidSignature` and HTTP `401`.
- Failed audience or role checks return `AuthError::Forbidden` and HTTP `403`.

## Current Limitations

- JWT signing is HS256 only in v0.
- Auth is stateless unless the application stores extra session or token data.
- Vyuh does not ship user models, registration, password reset, or account
  management flows.
- Role masks are limited to 64 bit positions.

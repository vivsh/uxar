# Assets

Vyuh assets are bundle-owned files that ship with a feature. They are used for
CSS, JavaScript, images, templates, SQL snippets, migrations, and other resource
files that belong beside the routes, services, tasks, and commands that use
them.

An asset dir is a structured resource root, not a plain static directory. Only
files under `public/` are web-accessible. Everything else is private framework
or application resource data.

## Overview

The main public pieces are:

- `#[bundles::asset_dir]` for registering a bundle asset directory.
- `embed_silo!("path")` for debug-filesystem and release-embedded assets.
- Runtime serving of `public/**` under `/assets`.
- `collect_static` for copying bundled public assets to a deployment directory.
- Minijinja loading of private `templates/**` files.

Asset dirs are part of bundle composition. A feature can register routes,
templates, public CSS, and private resources as one bundle.

## Directory Layout

Use convention folders inside each asset dir:

```text
assets/
  public/
    dashboard/
      dashboard.css
  templates/
    dashboard/
      layouts/
        base.html
  sql/
    reports.sql
  migrations/
    001_create_reports.sql
```

The folders have different visibility:

- `public/**` is served over HTTP and copied by `collect_static`.
- `templates/**` is loaded into Minijinja and is not public.
- `sql/**`, `migrations/**`, and other folders are private resources.

Public namespacing is done by folders under `public/`. For example,
`public/dashboard/dashboard.css` is served as `/assets/dashboard/dashboard.css`.

## Registration

Register an asset dir in a bundle:

```rust
use rust_silos::{Silo, embed_silo};
use vyuh::{bundles, embed};

const ASSETS: Silo = embed_silo!("assets");

#[bundles::asset_dir]
fn assets() -> embed::Dir {
    embed::Dir::new(ASSETS.clone())
}

let bundle = bundles::bundle! {
    assets,
};
```

`embed_silo!` reads from the filesystem in debug builds and embeds the files in
release builds. That keeps local frontend iteration fast while making release
binaries self-contained.

## Runtime Serving

Vyuh serves bundled public assets under `/assets` by default. The `public/`
prefix is stripped from the URL:

```text
public/dashboard/dashboard.css -> /assets/dashboard/dashboard.css
public/images/logo.svg -> /assets/images/logo.svg
```

Only `public/**` participates in runtime serving. Requests cannot reach
`templates/**`, `sql/**`, `migrations/**`, or other private folders through the
asset route.

`SiteConf.static_dir(...)` remains separate. Use configured static dirs for
application-owned filesystem folders. Use bundled assets for files that should
ship with a bundle and be available from debug filesystem reads or release
embedding.

## Templates

Minijinja templates are loaded from `templates/**`. The `templates/` prefix is
stripped when the template is registered:

```text
templates/dashboard/layouts/base.html -> dashboard/layouts/base.html
templates/dashboard/login.html -> dashboard/login.html
```

Template namespacing is done by folders under `templates/`. Public asset
namespacing is done by folders under `public/`. The two namespaces are
independent.

See [Templates](templates.md) for rendering APIs, template source rules, and
template failure modes.

A dashboard layout can refer to a public asset like this:

```html
<link rel="stylesheet" href="/assets/dashboard/dashboard.css" />
```

## Collect Static

`collect_static` copies all bundled `public/**` files to a target directory for
deployment through a CDN, reverse proxy, or dedicated static file host.

The destination path strips the `public/` prefix:

```text
public/dashboard/dashboard.css -> <output-dir>/dashboard/dashboard.css
public/images/logo.svg -> <output-dir>/images/logo.svg
```

`collect_static` does not copy templates, SQL files, migrations, or other
private resources. It copies the same public asset surface that runtime serving
exposes.

Use `collect_static` when the application server should not serve assets
directly in production, or when a deployment platform expects a static asset
directory.

## Debug And Release Behavior

Assets registered through `embed_silo!` have different storage behavior by build
mode:

- Debug builds read from the source filesystem.
- Release builds serve embedded bytes from the compiled binary.

The logical asset paths stay the same in both modes. A file such as
`public/dashboard/dashboard.css` is addressed as `/assets/dashboard/dashboard.css` whether it is
read from disk during development or served from the binary in production.

## Failure Modes

- Files outside `public/**` are not publicly served or collected.
- Missing public files return not found.
- Invalid paths and traversal attempts are rejected.
- Template names come from `templates/**`; public asset names come from
  `public/**`.
- `SiteConf.static_dir(...)` does not embed files and is independent from bundle
  assets.

## Current Limitations

- Asset dirs do not have package metadata.
- Public URL namespacing is folder-based under `public/`.
- Private resource folders are reserved for framework and application use; they
  are not exposed over HTTP.

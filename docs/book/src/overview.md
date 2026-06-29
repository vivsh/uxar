# Overview

Vyuh applications are built from ordinary Rust functions and a small runtime
model:

```text
Handler + Services + Assets -> Bundle
Bundle + Conf -> Site
Site -> Application + OpenAPI + Console
```

The goal is not to make every subsystem identical. The goal is to keep the
shape of the application visible: what enters, what runs, what it depends on,
and what the framework can expose for documentation and operations.

## Handlers

Handlers are the entrypoints into a Vyuh application. A route handles HTTP, a
command handles CLI or admin work, a task handles durable background work, and a
signal handler reacts to typed in-process events. Emitters produce scheduled or
external events and feed them back into the same handler model.

The common mental model is:

```rust
async fn handler(...context, Data<Input>) -> Result<Data<Output>, Error>
```

That is a guide, not a rigid signature. A handler can omit input, return `()`,
use auth or validation extractors, read path/query/json/form data, use
`ServiceRef<T>`, or return a lifecycle-specific type such as task state when
that runtime path needs it.

The important part is that handlers remain typed async functions. Their
signatures show the data and context they need.

## Services

Services are dependencies that handlers use. They are constructed once when the
site is built, stored for the lifetime of the site, and accessed through
`ServiceRef<T>` or `site.service::<T>()`.

A service is not an entrypoint. It is a reusable capability: a client, cache,
coordinator, repository, search index, mailer, or any other site-lifetime
component. Service methods can still follow the same typed input and output
style, but service construction belongs to the site lifecycle.

## Bundles

Bundles are the composition unit. A feature can own its routes, commands,
tasks, signals, emitters, services, templates, assets, and metadata together.

This keeps feature boundaries explicit. A bundle can be nested, reused,
prefixed, or published as a separate crate without scattering its runtime
paths and resources across unrelated configuration files.

Because a bundle is self-contained, applications can compose functionality by
importing bundles from other crates just as they import ordinary Rust
libraries. A blog, chat system, authentication module, or admin interface can
be packaged once and reused across multiple applications while remaining fully
integrated with routing, OpenAPI, the console, and the rest of the site.

## Site

`Site = Bundle + SiteConf`.

The site is the running application. It owns configuration, routing, services,
database access, tasks, signals, emitters, commands, templates, assets, logging,
console, OpenAPI, and shutdown coordination.

Because runtime paths are registered through bundles and typed handlers, Vyuh
can derive useful operational surfaces without extra per-feature wiring.
OpenAPI and console support come from the same application metadata that builds
the running site.

The practical result is one typed application core. HTTP APIs, background work,
commands, events, services, assets, docs, and operations stay connected instead
of becoming separate side systems.

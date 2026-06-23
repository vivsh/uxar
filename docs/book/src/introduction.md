# Vyuh

Vyuh is a Rust application framework for building typed runtime paths in one
place: routes, commands, tasks, signals, emitters, services, validation, assets,
templates, and OpenAPI.

The framework keeps application structure visible in ordinary Rust code. Handler
signatures describe what each runtime path consumes, `Site` owns the runtime,
and `Data<T>` gives typed application data a common shape across subsystems.

## What Vyuh Optimizes For

- A single application model across web and operational code.
- Explicit validation and error behavior.
- Bundle-owned routes, services, tasks, commands, assets, and templates.
- Postgres-first production behavior with MySQL and SQLite support where
  available.

## Current Status

Vyuh is usable, but not API-stable yet. Expect breaking changes before a stable
release.

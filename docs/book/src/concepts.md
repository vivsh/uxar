# Concepts

Vyuh is organized around a few small concepts.

## Site

`Site` is the built application. It owns configuration, database access,
templates, services, tasks, signals, emitters, commands, logging, and shutdown
coordination.

## Data

`Data<T>` is typed application data. Routes, commands, tasks, signals, and
emitters use it as the common data wrapper. Use `Valid<Data<T>>` when the
boundary should validate the parsed value.

## Bundles

Bundles are the composition unit. A feature can own its routes, services, tasks,
commands, signals, emitters, templates, and assets together.

## Services

Services are site-lifetime components. They are constructed when the site is
built and are accessed with `ServiceRef<T>` or `site.service::<T>()`.

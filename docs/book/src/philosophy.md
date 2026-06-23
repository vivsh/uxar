# Philosophy

Vyuh is built around one idea: real applications should not split their shape
across unrelated frameworks, scripts, queues, and service containers.

The framework gives web APIs, background work, commands, signals, emitters, and
services a shared application core. That core is meant to stay readable in
ordinary Rust code and explicit at every boundary.

## Ergonomic

Vyuh should keep the direct path direct.

Handlers, tasks, commands, and services should read like application code, not
like framework ceremony. The important things should be visible where the work
happens: what data enters, what validation applies, which service is used, and
what comes back.

Ergonomics in Vyuh means fewer rituals between intent and implementation.

## Cohesive

Vyuh treats application entry points as parts of one system.

A route, a task, a command, a signal handler, and a service are different
runtime paths, but they should not require different mental models. They should
share configuration, services, templates, assets, validation, errors, and
metadata through the same `Site`.

Cohesion means the project grows as one application, not as disconnected
adapters.

## Scalable

Vyuh should let projects grow without forcing a redesign at every milestone.

Small applications should stay small. Larger applications should compose
features through bundles, services, and runtime paths without losing structure.
The architecture should make it natural to split features by ownership while
keeping their routes, tasks, commands, templates, and assets together.

Scalability here is architectural: the project shape should survive growth.

## Observable

Vyuh makes runtime paths registered, inspectable, and documentable.

Routes, commands, tasks, signals, emitters, and services are not hidden behind
anonymous wiring. They contribute metadata the framework can use for OpenAPI,
console inspection, operational views, and documentation.

Observability starts with knowing what the application has registered.

## Safe By Type

Vyuh leans on Rust types at the boundary.

`Data<T>`, validation wrappers, schemas, typed extractors, typed services, and
explicit errors make behavior visible in signatures. The goal is not to hide
runtime failures, but to make the expected shape of data and dependencies clear
before the application runs.

Safety in Vyuh means important boundaries are typed, validated, and explicit.

## Complete

Vyuh is meant to cover the core application surface.

Web APIs, background tasks, commands, signals, emitters, services, templates,
assets, uploads, OpenAPI, logging, and console inspection belong in the same
framework because real applications need all of them together.

Completeness means fewer one-off side systems and more deliberate runtime paths.

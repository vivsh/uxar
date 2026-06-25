# Vyuh Web Agent Guide

For Bootstrap-based UI work in this folder, use the `$bootstrap-web-styleguide`
Codex skill. It contains the reusable styleguide for semantic HTML, Bootstrap
Sass usage, typography, CSS ownership, components, stylebook workflow, and
browser verification.

## Scope

- Web assets live under `vyuh/web/`.
- Shared public assets live under `vyuh/web/public/`.
- Runtime templates live under `vyuh/web/templates/` and are not public assets.
- Keep `node_modules/` uncommitted.

## Stylebook

- The dev-only stylebook lives at `vyuh/web/stylebook/index.html`.
- Open it locally with:
  `file:///Users/vivek/Projects/vyuh/vyuh/web/stylebook/index.html`.
- Keep the stylebook unlinked from public landing or mdBook navigation unless
  explicitly requested.

## SCSS Files

- `vyuh/web/scss/_overrides.scss`: Bootstrap overrides and inspectable design
  tokens.
- `vyuh/web/scss/_typography.scss`: semantic typography and named text
  patterns.
- `vyuh/web/scss/_components.scss`: reusable component structure, surfaces,
  spacing, borders, shadows, and states.
- `vyuh/web/scss/_layout.scss`: stylebook-only or page-specific layout
  treatment.
- `vyuh/web/scss/stylebook.scss`: stylebook import order.

## Build

- Install or update dependencies with `npm --prefix vyuh/web install`.
- Compile SCSS with `npm --prefix vyuh/web run build:css`.
- Do not edit `vyuh/web/public/css/stylebook.css` by hand.

## Vyuh Choices

- Headings use weight `500`.
- Emphasis, labels, captions, table headers, buttons, badges, and console nav
  links use the shared label weight.
- Named typography patterns include `vyuh-display`, `vyuh-section-title`,
  `vyuh-kicker`, and `vyuh-copy`.
- Prefer transparent logo assets for navigation and hero surfaces.

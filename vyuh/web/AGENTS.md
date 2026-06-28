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

- `vyuh/web/scss/_overrides.scss`: the single inspectable source for Bootstrap
  overrides, palette, type, spacing, radius, shadow, surface, status, control,
  table, and console scale tokens.
- `vyuh/web/scss/_typography.scss`: semantic typography and named text
  patterns. Prefer native `h1`-`h6`, `p`, `strong`, `caption`, `label`, `th`,
  `code`, and `pre` before adding local text classes.
- `vyuh/web/scss/_layout.scss`: shared root variables and structural layout
  primitives only. Do not put page-specific visuals here.
- `vyuh/web/scss/_components.scss`: compatibility import index for all reusable
  component partials. Prefer surface-specific imports in entrypoints.
- `vyuh/web/scss/components/_core.scss`: shared product primitives only. Do not
  put console, landing, docs, or stylebook selectors here.
- `vyuh/web/scss/components/_console.scss`: console-only component family.
- `vyuh/web/scss/components/`: reusable Vyuh component ownership. Components
  must not depend on Bootstrap utility clusters to render correctly.
- `vyuh/web/scss/pages/`: page-specific visuals such as the dev stylebook.
- `vyuh/web/scss/*-page.scss`: production surface rules for landing, docs, and
  console integration.
- `vyuh/web/scss/vyuh.scss`: the unified production entrypoint for landing,
  docs, console, and future public surfaces. Keep source modules split, but
  ship one shared production stylesheet for caching.

## Scale Rules

- New visual constants start in `_overrides.scss`, not in component files.
- Use the spacing, radius, shadow, surface, status, control, and table presets
  before adding a new raw value.
- Keep component partials small and focused by family: buttons, cards, forms,
  tables, badges, code, empty states, tabs, and console components.
- Bootstrap utilities are allowed for page composition only: containers, grid,
  flex, gaps, spacing between sections, and responsive visibility.
- If a utility cluster repeats or becomes required for component correctness,
  promote it into a named Vyuh class.
- Production pages share the unified `vyuh.scss` entrypoint. Keep style
  ownership modular in partials even though the public output is one CSS file.
- Keep `_typography.scss` semantic/global. Move `.landing-*`, `.console-*`, and
  `.stylebook-*` typography into their page or component partials.

## Build

- Install or update dependencies with `npm --prefix vyuh/web install`.
- Compile SCSS with `npm --prefix vyuh/web run build:css`.
- Check generated CSS quality with `npm --prefix vyuh/web run check:css`.
- Do not edit generated CSS under `vyuh/web/public/css/` by hand.
- `build:css` writes `vyuh.css`, `vyuh.<hash>.css`, and `manifest.json`.

## Generated CSS Review

- Generated CSS is an audit artifact, never a hand-edit target.
- `vyuh.css` is the single production bundle and may contain landing, docs, and
  console selectors.
- `vyuh.css` must not contain `.stylebook-` selectors.
- `manifest.json` must map `vyuh.css` to a generated `vyuh.<hash>.css` file.
- Large `!important` counts are acceptable only when they come from Bootstrap
  utilities.

## Vyuh Choices

- Headings use weight `500`.
- Emphasis, labels, captions, table headers, buttons, badges, and console nav
  links use the shared label weight.
- Named typography patterns include `vyuh-display`, `vyuh-section-title`,
  `vyuh-kicker`, and `vyuh-copy`.
- Prefer transparent logo assets for navigation and hero surfaces.

# Repository Guidelines

## Project Structure & Module Organization

Verditum is a Tauri 2 document viewer for UYAP `.udf` files. The Lit/TypeScript
frontend styles native elements with [Open Props](https://open-props.style)
tokens; Rust extracts `content.xml` from document archives and sends it to the
UI.

- `src/main.ts`: `MainView`, document display, theme selection, and zoom
  controls.
- `src/udf/`: safe XML-to-DOM formatting and document styles; `test/`: rendering
  tests.
- `src/styles.css` and `src/assets/`: Open Props imports, light and dark theme
  tokens, and UI SVGs, taken from
  [Bootstrap Icons](https://icons.getbootstrap.com/). Only custom properties
  cross into `MainView`'s shadow root, so its controls are styled there.
- `src-tauri/src/lib.rs`: native commands, file handling, plugins, and
  application lifecycle; `main.rs` starts the desktop app.
- `src-tauri/src/pdf/`: PDF export. `model.rs` parses `content.xml` into
  resolved elements, `fonts.rs` subsets and embeds system fonts, and `mod.rs`
  lays out pages and writes the file. `lib.rs` opens the result with
  `tauri-plugin-opener` on desktop and `tauri-plugin-view` on mobile.
- `src-tauri/src/update.rs`: desktop auto-update. Release builds check the
  latest GitHub release's `latest.json` at startup and install with consent.
- `src-tauri/tauri.conf.json` and `capabilities/`: application configuration and
  permissions. `tauri.updater.conf.json` enables signed updater packages in the
  release workflow, whose assets and `latest.json` come from
  `scripts/release-assets.mjs`.
- `assets/`: source application icons; `src-tauri/icons/`: bundled icons.
- `dist/`, `src-tauri/target/`, and `src-tauri/gen/`: generated output.

## Build, Test, and Development Commands

- `npm ci`: install frontend dependencies from the lockfile.
- `npm run dev`: start Vite on port 1420 for frontend development.
- `npm run debug`: launch the native Tauri app with the Vite server; requires
  Rust and platform build prerequisites.
- `npm run build`: type-check TypeScript and build frontend assets into `dist/`.
- `npm run typecheck`: type-check application and test code.
- `npm run format`: format frontend code and documents with Prettier, and Rust
  code with nightly rustfmt.
- `npm run preview`: serve the frontend production build locally.
- `npm run release`: package a native build for the host platform.
- `cd src-tauri && cargo test`: run the Rust unit tests, including PDF export.
- `cd src-tauri && cargo +nightly fmt --check`: check Rust formatting; nightly
  is required for the configured unstable options.

`npm run clean` invokes `git clean -Xfd`, removing ignored files such as
`node_modules`, build output, and generated mobile projects; untracked files
that are not ignored are kept.

## Coding Style & Naming Conventions

Use two-space indentation, including Rust as configured in `.rustfmt.toml`.
Match TypeScript's double quotes and semicolons. Use PascalCase for
classes/types, camelCase for TypeScript members, snake_case for Rust functions,
and kebab-case for custom elements such as `main-view`. Preserve strict
TypeScript checks. Prettier uses an 80-column print width and wraps prose. No
dedicated JavaScript linter is configured.

## Testing Guidelines

Use Vitest with Playwright Chromium for browser and renderer tests in
`test/*.test.ts`; Node unit tests belong in `test/unit/**/*.test.ts`. Install
Chromium with `npx playwright install chromium`. Run `npm test` (which first
type-checks application and test code) and `npm run test:coverage`
(`npm run coverage` is an alias); coverage includes all TypeScript source files
and no threshold is enforced. Run `npm run build` before submitting. Rust tests
live beside the code they cover in `#[cfg(test)]` modules; run them with
`cargo test` from `src-tauri`. Cover text offsets, style inheritance, element
order, and unsafe input when changing rendering, and pagination, warnings, and
rejected input when changing PDF export. Manually verify native file opening,
zoom, theme persistence, print output, and PDF export.

## Commit & Pull Request Guidelines

History uses brief subjects such as `release intel mac`; no formal commit
convention is evident. Write concise, descriptive subjects. PRs should explain
the change, link relevant issues, list validation and target platforms, and
include screenshots for UI changes.

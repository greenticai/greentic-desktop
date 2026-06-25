# PR-35 - Embed Automate Hub Frontend Assets

## Goal

Bring the attached Greentic Automate Hub React interface into this repository as the default graphical user interface for `greentic-desktop`, without wiring backend actions yet. This PR creates the frontend project boundary, build pipeline, embedded static assets, and CI checks needed by later GUI/API PRs.

## User Outcome

When a user installs `greentic-desktop`, the binary contains the Automate Hub UI assets. The UI can be served locally from the Rust runtime without requiring Node, Bun, Vite, or source files on the user's machine.

## Current State

- The attached zip contains a Vite/TanStack React app with routes:
  - `/` home and setup checklist
  - `/create` prompt and recording wizards
  - `/runners` runner management cards
  - `/mcp` published MCP tools
  - `/settings` setup, extensions, LLM, and advanced settings
- The app currently uses `src/lib/demo-data.ts` for sessions, extensions, runners, MCP tools, recordings, approvals, and activity.
- The Rust workspace has no frontend crate, static asset embedding, GUI mode, or browser-launch behavior.

## Scope

1. Add a frontend source tree under a clear path such as `frontend/automate-hub/`.
2. Preserve the attached app structure initially:
   - `package.json`
   - `bun.lock`
   - `src/routes/*`
   - `src/components/*`
   - `src/styles.css`
   - Vite/TanStack config
3. Replace project metadata:
   - package name should be `greentic-automate-hub`
   - remove Lovable-only project markers from release assets if they are not required at runtime
   - keep third-party UI dependencies pinned through the committed lockfile
4. Add a frontend build output convention, for example:
   - `frontend/automate-hub/dist/`
   - generated files are ignored
   - Rust embeds from generated assets during release builds
5. Add a Rust crate such as `crates/greentic-desktop-gui-assets` or a module in a future GUI crate that exposes:
   - `index.html`
   - static JS/CSS/favicon assets
   - content type lookup
   - content hash or build version
6. Add build documentation for frontend contributors.

## Non-Goals

- Do not wire live backend API calls in this PR.
- Do not change the CLI behavior yet.
- Do not require a system webview framework; this UI will be browser-served first.

## Technical Plan

### Frontend Placement

Use `frontend/automate-hub` rather than placing TypeScript inside `crates/`. This keeps Rust crates clean while making the UI build explicit.

Recommended tree:

```text
frontend/automate-hub/
  package.json
  bun.lock
  src/
  public/
  vite.config.ts
  tsconfig.json
```

### Static Build

Configure the UI build to produce a static single-page app. The router must work when served from the local Rust HTTP host:

- direct navigation to `/create`, `/runners`, `/mcp`, and `/settings` should return `index.html`
- assets should use relative or root-local paths that work on `http://127.0.0.1:<port>/`
- no external runtime dependency on the Vite dev server

### Rust Asset Embedding

Evaluate one of these options:

- `include_dir` for compile-time embedding
- generated Rust module produced by a build script
- serving unpacked assets in debug mode and embedded assets in release mode

The implementation should prefer a simple compile-time embedding in release builds so a downloaded `.exe` can run without extra files.

### CI

Add a frontend validation step:

- install dependencies through Bun or npm, whichever the project standardizes on
- run typecheck/lint if available
- build static assets
- verify `dist/index.html` exists

The CI step must be isolated so Rust-only checks remain understandable.

## Acceptance Criteria

- The UI source from the attached bundle is present in the repository in a maintainable location.
- `README.md` or `docs/developer-notes.md` explains how to build the frontend.
- A frontend build command produces static assets.
- Rust code can reference embedded assets through a small typed asset API.
- Generated `dist/` assets are not accidentally committed unless the team explicitly chooses checked-in assets.
- Existing `ci/local_check.sh` still passes or has a clear frontend-aware extension.

## Test Plan

- Run frontend install/build.
- Run static route smoke tests against built files.
- Run `cargo test --all-features`.
- Run `bash ci/local_check.sh`.

## Risks

- React 19/TanStack Start/Vite versions may need lockfile-driven reproducibility.
- Embedding all assets increases binary size, though this UI appears small enough for direct embedding.
- If the app relies on TanStack Start server behavior, it must be converted to a static SPA shape before embedding.


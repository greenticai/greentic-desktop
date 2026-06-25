# Greentic Automate Hub Frontend

This directory contains the browser UI that will be served by `greentic-desktop` GUI mode.

The source was imported from the Greentic Automate Hub bundle and is kept separate from the Rust crates so the UI can be built, linted, and iterated on independently.

## Commands

```bash
npm ci
npm run lint
npm run build
```

The build output is written to `dist/` and is intentionally ignored by Git. The Rust `greentic-desktop-gui-assets` crate embeds this directory when it is present during compilation, so release builds should run the frontend build before compiling the desktop binary.

## Current State

PR-35 imports the UI source and establishes the build/output convention. The current route data is still demo-backed. Later PRs replace that data with the local Greentic Desktop JSON API.

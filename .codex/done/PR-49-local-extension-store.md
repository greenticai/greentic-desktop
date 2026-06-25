# PR-49 - Local Extension Store, Install, Update and Remove

Goal: implement the local installed-extension lifecycle.

This is the PR that makes install actually work end-to-end.

Local extension store
~/.greentic/desktop/extensions/
  installed.lock
  greentic.desktop.playwright/
    1.0.0/
      extension.toml
      manifest.cbor
      bin/
      sidecar/
      assets/
    current -> 1.0.0
Commands
gtc desktop extension install greentic.desktop.playwright
gtc desktop extension list
gtc desktop extension info greentic.desktop.playwright
gtc desktop extension update greentic.desktop.playwright
gtc desktop extension remove greentic.desktop.playwright
gtc desktop extension verify greentic.desktop.playwright
gtc desktop extension health greentic.desktop.playwright
Lock file
[[extensions]]
id = "greentic.desktop.playwright"
version = "1.0.0"
source = "oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0"
digest = "sha256:..."
installed_at = "2026-06-25T10:30:00Z"
enabled = true

GUI integration
The Automate Hub Settings > Extensions page must read this local store for installed version, enabled/disabled state, health status, source URI, digest and update availability. Install, update, remove, enable, disable, verify and health actions in the GUI must mutate this same store and lock file.

Acceptance criteria
Extension can be installed into the local extension store.
Installed extension is added to installed.lock.
Extension can be updated, removed, enabled and disabled.
Extension capabilities are discovered after install.
Extension health check can be run.
Broken installs roll back cleanly.

# PR-48 - greentic-distributor-client Integration for Extensions

Goal: use greentic-distributor-client as the only download/resolution layer.

This avoids writing custom GHCR/download logic inside greentic-desktop.

Supported URI schemes
oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0
store://greentic.desktop.playwright
repo://tenant/extensions/playwright
file://./extensions/playwright.extension.tar.zst
Resolver flow
extension install request
  → parse URI or extension ID
  → resolve alias if needed
  → call greentic-distributor-client
  → download artifact
  → verify digest/signature
  → return local bundle path
CLI examples
gtc desktop extension install greentic.desktop.playwright

gtc desktop extension install store://greentic.desktop.playwright

gtc desktop extension install oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0

gtc desktop extension install file://./playwright.extension.tar.zst

GUI integration
The GUI extension install API must call this same resolver path. The frontend should receive progress phases such as resolving, downloading, verifying and failed, but it must not implement store, OCI, repo or file download logic itself.

Acceptance criteria
Extension manager does not implement registry clients directly.
All remote extension downloads go through greentic-distributor-client.
oci://, store://, repo://, and file:// are supported.
Resolved artifacts include digest, version, source URI and local cache path.
Failed downloads produce user-friendly diagnostics.

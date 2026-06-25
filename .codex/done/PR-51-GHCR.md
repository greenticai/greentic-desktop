# PR-51 - Extension Build and Publish Pipeline to GHCR

Goal: publish official extensions as OCI packages on GHCR.

Example packages
ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/windows-uia:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/macos-ax:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/linux-x11:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/java-accessibility:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/vision-fallback:1.0.0
CI flow
build extension
  → validate manifest
  → run tests
  → generate SBOM
  → package extension
  → sign artifact
  → publish to GHCR
  → update store index
Acceptance criteria
CI can package extensions as OCI artifacts.
GHCR publish works for tagged releases.
Each package includes manifest, capabilities, permissions and SBOM.
Published package digest is recorded.
GUI integration
The GUI should not talk to GHCR directly. It consumes the store index updated by this pipeline through `/api/v1/extensions/recommended`, search and versions endpoints. The UI may show GHCR source/digest in advanced details for debugging and auditability.

Acceptance criteria
CI can package extensions as OCI artifacts.
GHCR publish works for tagged releases.
Each package includes manifest, capabilities, permissions and SBOM.
Published package digest is recorded.
Store index is updated with the new version.

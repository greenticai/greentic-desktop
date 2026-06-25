# PR-47 - Extension Package Format and OCI Artifact Layout

Goal: define what a Greentic Desktop extension package is.

This PR should standardise the on-disk and OCI artifact format.

Package layout
extension.tar.zst
  extension.toml
  manifest.cbor
  permissions.cbor
  capabilities.cbor
  bin/
    adapter
  sidecar/
    package.json
    index.js
  assets/
  schemas/
  examples/
  README.md
  SBOM.spdx.json
  signatures/
extension.toml
id = "greentic.desktop.playwright"
name = "Playwright Web Adapter"
version = "1.0.0"
publisher = "greenticai"
runtime = "sidecar"
entrypoint = "sidecar/index.js"

[distribution]
source = "oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0"

[platforms]
windows = true
macos = true
linux = true

[capabilities]
tools = [
  "web.goto",
  "web.click",
  "web.fill",
  "web.extract_text",
  "web.assert_visible",
  "evidence.screenshot"
]

[permissions]
network = true
filesystem = "limited"
screen_capture = false
keyboard_mouse = false
Acceptance criteria
A Greentic extension package has a documented package layout.
Package metadata includes ID, version, runtime, platforms, capabilities and permissions.
Package format supports native and sidecar extensions.
Package can be represented as an OCI artifact.
Package includes SBOM/provenance placeholders.
Extension manifest can be validated before install.

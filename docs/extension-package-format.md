# Extension Package Format

A Greentic Desktop extension package is distributed as a compressed archive, usually `extension.tar.zst`, and can also be stored as an OCI artifact.

## Archive Layout

```text
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
```

Native extensions place executable payloads under `bin/`. Sidecar extensions place their runtime entrypoint under `sidecar/`. Packages should include README, SBOM, provenance, and signature placeholders even when a development package is unsigned.

## `extension.toml`

```toml
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
  "evidence.screenshot",
]

[permissions]
network = true
filesystem = "limited"
screen_capture = false
keyboard_mouse = false
```

Required metadata:

- `id`, `name`, `version`, and `publisher`
- `runtime`: `native` or `sidecar`
- `entrypoint` for sidecar packages
- supported platforms
- declared capabilities
- permissions requested by the package
- distribution source when installed from a store or registry

## OCI Artifact

The package archive can be pushed as an OCI artifact with:

- layer media type `application/vnd.greentic.desktop.extension.layer.v1+tar+zstd`
- manifest media type `application/vnd.greentic.desktop.extension.manifest.v1+json`
- annotations for extension ID, version, publisher, runtime, and supported platforms

The digest from the OCI registry is part of the trust decision shown in Automate Hub and stored in the local extension state.

## Validation

Before install, the runtime validates:

- required metadata is present,
- at least one platform is supported,
- sidecar packages declare an entrypoint,
- capabilities are not empty,
- distribution source uses an accepted scheme such as `oci://`, `store://`, or `file://`,
- signatures, SBOM, publisher trust, and permissions satisfy local policy.

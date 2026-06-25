# Extension Store

Automate Hub Settings shows recommended extensions and installed local extensions. Extension IDs are stable friendly IDs such as `greentic.desktop.playwright`, `greentic.desktop.vision`, and `greentic.desktop.terminal.tn3270`.

## Sources

Extensions can come from:

- recommended built-in manifests,
- store aliases such as `store://greentic.desktop.playwright`,
- GHCR-backed OCI artifacts for packaged extensions,
- local files during development.

Remote OCI artifacts should carry source URI, digest, publisher, signature status, permissions, capabilities, and SBOM metadata. Local draft extensions are intended for development and should not be treated as production-trusted packages.

The package layout and OCI media types are defined in [Extension Package Format](extension-package-format.md).
Official release publishing is described in [Extension GHCR Publish Pipeline](extension-ghcr-pipeline.md).

## Distributor Resolution

All extension installs go through `greentic-distributor-client`. The GUI and CLI pass the requested ID or URI to the resolver; they do not implement GHCR, store, repo, or file download logic themselves.

Supported request forms:

```text
greentic.desktop.playwright
store://greentic.desktop.playwright
oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0
repo://tenant/extensions/playwright
file://./playwright.extension.tar.zst
```

Resolution returns:

- extension ID,
- version,
- original source URI,
- resolved artifact URI,
- digest,
- local cache path,
- progress phases such as resolving, downloading, and verifying.

The default store index supports friendly aliases. For example, `playwright`, `browser`, and `web` resolve to `greentic.desktop.playwright`, which resolves to `store://greentic.desktop.playwright`, then to the GHCR OCI artifact for the latest version.

CLI helpers:

```bash
greentic-desktop extension search browser
greentic-desktop extension install playwright
greentic-desktop extension versions greentic.desktop.playwright
```

## Install, Update, Remove

From **Settings**, search or browse recommended extensions, then use install, update, enable, disable, verify, health, or remove. The backend remains authoritative: bypassing the frontend cannot skip trust policy, signature, or permission checks.

## Permissions

High-impact permissions require clear approval before production use:

- screen capture,
- keyboard and mouse control,
- filesystem write,
- network access,
- native sidecar execution.

The UI should show permission prompts and advanced details before installation. The runtime should block untrusted publishers, unsigned production packages, and high-risk permissions unless policy explicitly allows them.

## Local Store State

Installed extension state lives under the Greentic Desktop runtime home. The setup checklist and installed-extension list read from that local state so a restart preserves extension availability.

```text
~/.greentic/desktop/extensions/
  installed.lock
  greentic.desktop.playwright/
    1.0.0/
      extension.toml
      manifest.cbor
      bin/
      sidecar/
      assets/
    current
```

`installed.lock` records ID, version, source URI, digest, install time, and enabled state. Install and update write a new version directory before changing the current pointer and lock entry. Remove deletes the extension directory and lock entry. Enable and disable mutate only the lock state.

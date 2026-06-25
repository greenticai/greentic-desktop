# Extension GHCR Publish Pipeline

Official extensions are packaged during tagged releases and published as OCI artifacts to GHCR.

Example artifact refs:

```text
ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/windows-uia:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/macos-ax:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/linux-x11:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/java-accessibility:1.0.0
ghcr.io/greenticai/greentic-desktop/extensions/vision-fallback:1.0.0
```

The release workflow:

1. builds an extension package,
2. validates required package files,
3. includes manifest, permissions, capabilities, README, signatures, and SBOM placeholders,
4. pushes the artifact with ORAS,
5. records the published digest,
6. emits a store-index fragment containing ID, version, source, and digest.

Automate Hub does not call GHCR directly. It reads recommended extensions, search results, versions, source URI, and digest through the local GUI API backed by the store index and distributor client.

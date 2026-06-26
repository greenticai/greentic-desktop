# PR-62 - Store Index Generation from Extension Manifests

Goal: eliminate drift between built-in extension manifests and the distributor store index by generating store entries from manifest metadata.

Problem
The store index duplicates extension IDs, aliases, capabilities, platforms, and permissions. It already differs from built-in manifests in places such as terminal capabilities. This creates install, planning, and capability-routing inconsistencies.

Design
Make built-in extension manifests the source of truth. Add store metadata to extension definitions or create a generated conversion:

- manifest ID
- name
- version
- runtime
- capabilities
- permissions
- platforms
- aliases
- publisher
- source template
- latest version

Implementation options

Option A: extend `ExtensionManifest` with store metadata fields.
Option B: add `BuiltInExtensionCatalogEntry` wrapping manifest + aliases/platforms/source.
Option C: generate store index from a static table that references manifest IDs and derives capabilities/permissions from `built_in_extension`.

The distributor client should call the generated catalog instead of hand-maintaining a separate extension list.

Acceptance criteria
Store capabilities match built-in manifest capabilities for every built-in extension.
Store permissions match or intentionally summarize manifest permissions with a test documenting the mapping.
Aliases are defined once.
Adding a new built-in extension requires updating one catalog entry, not multiple duplicated lists.
Tests fail if store index and built-in manifests drift.

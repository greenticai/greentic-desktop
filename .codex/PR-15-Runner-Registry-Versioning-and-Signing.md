# PR-15 Runner Registry, Versioning and Signing

## Goal

Store, approve, version and distribute runners.

## Registry Responsibilities

- Store runner packages
- Manage lifecycle
- Sign published packages
- Support promotion dev → staging → prod
- Support rollback to previous versions
- Support tenant/team scoping
- Support private registries

## Lifecycle

```text
draft
  → tested
  → approved
  → published
  → deprecated
  → archived
```

## Versioning

Use semantic versions:

```text
crm.create_customer@1.2.0
crm.create_customer@stable
crm.create_customer@dev
```

Production flows should pin exact versions unless policy allows channels.

## Commands

```bash
gtc desktop runner pull crm.create_customer --version 1.2.0
gtc desktop runner publish crm.create_customer --version 1.2.0
gtc desktop runner promote crm.create_customer --from staging --to prod
gtc desktop runner verify crm.create_customer
```

## Signing

The manifest is signed.

The runtime verifies:

- Signature
- Tenant scope
- Runner version
- Required adapters
- Policy
- Compatibility

## Acceptance Criteria

- Published runners must be signed.
- Runtime refuses tampered runner packages.
- Runner packages are Git-friendly.
- Version diffs are reviewable.

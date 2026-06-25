# PR-23 Deployment, Updates and Airgapped Support

## Goal

Support connected and airgapped deployment of the Desktop Runner, extensions and approved runner packages.

## Connected Mode

```text
gtc desktop extension install greentic.desktop.playwright
gtc desktop runner pull crm.create_customer --version stable
```

## Airgapped Mode

```text
public updater exports:
  - runtime update
  - extension bundle
  - runner packages

admin copies to airgapped environment

desktop runner imports and verifies:
  - signatures
  - checksums
  - tenant scope
  - compatibility
```

## Update Package Types

- Runtime update
- Extension update
- Runner package update
- Policy update
- Revocation list

## Security

- Signed packages
- mTLS for connected pulls
- Local signature verification
- Revocation list
- Audit logs

## Acceptance Criteria

- Runtime can install from online registry.
- Runtime can install from local bundle.
- Unsigned packages are rejected.
- Extension and runner dependencies are checked.

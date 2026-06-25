# Deployment and Rollout

Greentic Desktop includes models for safely distributing runtime updates, extensions, runner packages, policies, and revocation lists.

## Deployment Modes

Two deployment modes are modeled:

- **Connected**: the desktop can reach a central source for updates.
- **Airgapped**: updates are delivered as signed local bundles.

## Update Package Types

Deployment packages can contain:

- runtime updates,
- extension updates,
- signed runner packages,
- policies,
- revocation lists.

Each package carries compatibility information, dependencies, signatures, audit details, and rollback actions.

## Revocation

Revocation lists let administrators block packages that should no longer be trusted. A revoked package should not be installed or used as part of a production run.

## Rollout Validation

Rollout flows model canary-style validation. A patch or desktop image update can be tested by running approved validation runners and collecting evidence reports.

A rollout decision can approve the next ring, block rollout, or trigger rollback actions based on runner results and failure thresholds.

## AWS WorkSpaces

The repository also models AWS WorkSpaces integration for installing the runtime, pulling approved runners, exposing MCP tools, and forwarding runner calls to a workspace with evidence.

See [AWS WorkSpaces MCP Forwarding](aws-workspaces-mcp.md) for the end-to-end flow from prompt-recorded runner to AWS-forwarded MCP tool.

# Security, Approvals, and Secrets

Greentic Desktop treats desktop automation as a controlled action. Runners are checked against policy before they run.

## Default Security Posture

The default runtime configuration:

- requires signed published runners,
- allows unsigned drafts for local authoring,
- requires signed extensions,
- binds the MCP endpoint to localhost,
- stores evidence under the Greentic Desktop home directory.

## Action Permissions

Policy controls whether a runner may:

- read the screen,
- type text,
- submit forms,
- delete records,
- make payments,
- perform bulk updates.

By default, reading, typing, and form submission are allowed. Deleting records, payments, and bulk updates are blocked unless policy explicitly allows them.

## Risk Levels And Approvals

Runners can be assigned a risk level. Policy can require:

- approval before production use,
- one approval for high-risk actions,
- multiple approvals for critical actions,
- approval before bulk updates.

If approval is missing, Greentic Desktop returns a structured denial instead of running the task.

## Environment Allow Lists

Policies can restrict where a runner is allowed to execute. The default environment list includes:

- `dev`
- `staging`
- `production`

Teams can narrow this list for sensitive workflows.

## Secrets

Secrets are passed separately from ordinary inputs. Runner steps can refer to secrets without storing the secret value in the runner package.

Greentic Desktop also redacts sensitive text in evidence-oriented paths. Text that looks like a password, token, secret, or secret reference is replaced with `[REDACTED]`.

## Signed Runners And Extensions

Published runners and extensions are expected to be signed. This prevents unreviewed or tampered automation from being treated as production-ready.

GLOBAL RULE - REPO OVERVIEW, CI, AND REUSE OF GREENTIC REPOS

For THIS REPOSITORY, always:

1. Maintain `.codex/repo_overview.md` using the Repo Overview Maintenance routine before starting PR-style work and after finishing it.
2. Run `ci/local_check.sh` at the end of PR-style work and ensure it passes, or explain precisely why it cannot be made to pass as part of the PR.
3. Prefer existing Greentic repos/crates for interfaces, types, secrets, oauth, messaging, events, and other shared behavior instead of reinventing them locally.

## Workflow for Every PR

1. Pre-PR sync:
   - Check out the target branch, usually the default branch unless another branch is specified.
   - Fully refresh `.codex/repo_overview.md` before making code changes.
   - Show the updated overview if it changed in a meaningful way.

2. Implement the PR:
   - Apply requested code, tests, docs, and config changes.
   - Before adding new core types, interfaces, or cross-cutting functionality, check whether they already exist in other Greentic repos such as `greentic-interfaces`, `greentic-types`, `greentic-secrets`, `greentic-oauth`, `greentic-messaging`, or `greentic-events`.
   - Use a suitable shared type or interface when one exists.
   - Do not fork or duplicate cross-repo models without a clear documented reason.
   - Run appropriate build and test commands while working.

3. Post-PR sync:
   - Refresh `.codex/repo_overview.md` against the updated codebase.
   - Run `ci/local_check.sh` from the repo root.
   - Fix failures caused by the work.
   - If failures are outside the change scope, capture the failing steps and key errors in the final summary.

## Behavioral Rules

- Do not ask for permission to run the repo overview routine, run `ci/local_check.sh`, or reuse existing Greentic crates.
- Never leave `.codex/repo_overview.md` partially updated or obviously inconsistent.
- Never introduce new core types or interfaces that duplicate shared Greentic crates without a strong documented justification.
- If i18n support is requested, follow the Greentic i18n CLI playbook and adapt `tools/i18n.sh` from `greentic-component`.

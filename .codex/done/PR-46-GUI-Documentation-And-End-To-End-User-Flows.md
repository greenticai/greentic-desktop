# PR-46 - GUI Documentation and End-to-End User Flows

## Goal

Document the new GUI-first Greentic Desktop experience and add end-to-end validation flows covering prompt, recording, remote extension install, runner validation, MCP exposure, and install/start behavior.

## User Outcome

A user can install or download Greentic Desktop, launch the GUI, record or prompt an automation, validate it, publish it as an MCP tool, and use it from an AI client or Greentic flow by following clear documentation.

## Current State

- README and docs explain CLI-first workflows.
- Existing docs cover recording, prompt planning, MCP, AWS WorkSpaces, adapters, and install.
- No GUI-specific docs or screenshots exist.

## Scope

1. Update README to describe GUI-first startup.
2. Add `docs/gui.md`.
3. Update `docs/getting-started.md` with browser GUI flow.
4. Update `docs/recording-and-refinement.md` with GUI recording steps.
5. Update `docs/runners.md` with GUI runner test/publish actions.
6. Update `docs/mcp-tools.md` with GUI MCP server controls.
7. Update `docs/aws-workspaces-mcp.md` with GUI-driven setup references.
8. Add extension store documentation covering recommended extensions, search, install/update/remove, permissions, trust policy, GHCR/OCI sources, and local file installs.
9. Add end-to-end smoke test documentation.

## Documentation Outline

### README

- "Open Greentic Desktop"
- "Create your first automation"
- "Test and save"
- "Publish as MCP"
- "Use from AI workers"
- Link to detailed GUI docs

### docs/gui.md

Include:

- launch behavior
- home/setup checklist
- create from prompt
- record a task
- runner management
- MCP tools page
- settings/extensions/permissions
- extension store, installation progress, update/remove, and trust prompts
- troubleshooting browser did not open
- logs location

### docs/getting-started.md

Add a GUI-first path before CLI commands:

1. Install or download.
2. Start `greentic-desktop` or double-click on Windows.
3. Complete setup checklist.
4. Install or verify required extensions from Settings.
5. Create runner from prompt.
6. Test.
7. Publish as MCP.

### docs/cli-reference.md

Clarify:

- `greentic-desktop` starts GUI by default.
- CLI commands are still available.
- `gtc desktop` remains the explicit CLI namespace.

## End-to-End Validation Plan

Add a manual or automated checklist:

- fresh runtime home
- launch GUI
- setup checklist loads
- create prompt runner
- save runner
- runner appears in list
- test runner
- publish as MCP
- MCP tool appears
- call tool and view evidence
- install/update/remove an extension from the GUI
- blocked extension install shows trust-policy reason
- restart app and verify state persists

## Acceptance Criteria

- README is GUI-first while still documenting CLI paths.
- Detailed GUI docs exist.
- Existing recording/prompt/MCP docs link to GUI flow.
- Troubleshooting covers browser launch, permissions, MCP bind conflicts, and private-release/bininstall caveats.
- Extension documentation explains friendly IDs, store aliases, GHCR-backed OCI artifacts, local file installs, trust prompts, and local extension store state.
- End-to-end checklist is usable by QA and release validation.

## Test Plan

- Link check for docs.
- Manual run through GUI-first getting-started doc.
- Validate docs match actual UI labels and commands after implementation.

## Risks

- Docs can drift while GUI PRs land. Keep this PR after core GUI/API implementation and update screenshots/text last.

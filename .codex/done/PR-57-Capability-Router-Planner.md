# PR-57 - Capability-Router Planner

Goal: replace keyword-based planning with a generic capability router that chooses technologies and workflow strategies from installed adapters, app metadata, observations, session profiles, existing runners, and prompt intent.

Problem
The planner currently uses prompt keywords such as CRM, company name, customer ID, mainframe, and terminal. This is not robust enough for arbitrary desktop automations.

Design
Add a capability router pipeline:

1. Parse task intent.
2. Detect target environment from prompt, app metadata, active window observations, URLs, process names, session profile, or user selection.
3. Match required actions to installed adapter capabilities.
4. Prefer structured adapters over vision fallback.
5. Reuse existing runners or MCP tools when possible.
6. Produce a `DesktopWorkflow`.
7. Compile to runner steps.
8. Return open questions when confidence is insufficient.

Routing signals
Use these inputs:

- installed adapters and capabilities
- desktop observations and UI trees
- browser URL/page metadata
- active process/window metadata
- terminal profile metadata
- Java Accessibility availability
- Wayland limitations
- prior successful runners
- user-selected target app
- policy constraints

Adapter selection policy
Preferred order:

1. existing runner or MCP tool that satisfies intent
2. web adapter for browser DOM tasks
3. Java adapter for Java accessibility tasks
4. native OS adapter for native app UI trees
5. terminal adapter for terminal/mainframe sessions
6. vision adapter for fallback or cross-checks
7. ask clarification if multiple routes are plausible

Acceptance criteria
Planner no longer infers inputs/outputs from CRM-specific keywords.
Planner can choose web, native, Java, terminal, or vision based on capabilities and observations.
Planner emits open questions for unknown app target, unknown credential, unknown output, or unsafe submit.
Planner can recommend installing a missing adapter.
Planner tests cover at least five different app technologies and no test depends on CRM as the only example.

# Greentic Desktop Runner — Detailed PR Pack

This package defines an implementation-ready roadmap for a generic Greentic Desktop Runner.

The design goal is:

```text
Prompt → Interactive desktop execution → User refinement → Recorded runner → Versioned approval → MCP tool publication → Reuse in Greentic flows, AWS WorkSpaces, or other MCP clients
```

The Desktop Runner is intentionally generic. AWS WorkSpaces is one execution environment, not the architecture itself.

## Main Concepts

- **Desktop Runner Runtime**: Rust executable that hosts adapters, sessions, recording, replay and MCP.
- **Adapters**: Web, Windows UI, Java, terminal/mainframe, Office, vision fallback.
- **Runner Package**: Versioned, signed, Git-friendly recorded automation capability.
- **Prompt Builder**: Converts user prompts into executable runner drafts.
- **Interactive Refinement**: Users correct failed steps by prompting.
- **MCP Publishing**: Approved runners become typed MCP tools.
- **LTM**: Stores run history, failures, fixes, screenshots, app versions and root causes.
- **Workspace Worker**: Uses runners for patch validation, rollout, rollback and business processes.

## PR Index

1. PR-01 Core Runtime and CLI
2. PR-02 Adapter SDK and Capability Model
3. PR-03 Extension Manager and Sidecar Runtime
4. PR-04 Playwright Web Adapter
5. PR-05 Windows UI Adapter
6. PR-06 Java Desktop Adapter
7. PR-07 Terminal/Mainframe Adapters
8. PR-08 Vision and Screenshot Fallback Adapter
9. PR-09 Session Bootstrap Profiles
10. PR-10 Recording Engine and Portable Runner Package
11. PR-11 Prompt-to-Runner Planner
12. PR-12 Interactive Refinement Loop
13. PR-13 Replay Engine and Validation
14. PR-14 Evidence Store and Audit Bundles
15. PR-15 Runner Registry, Versioning and Signing
16. PR-16 MCP Server and Tool Publishing
17. PR-17 Security, Secrets and Policy Enforcement
18. PR-18 LTM and Root Cause Learning
19. PR-19 AWS WorkSpaces Integration
20. PR-20 Workspace Worker Patch/Test/Rollout Flows
21. PR-21 Business Process Automation with Desktop Runners
22. PR-22 Forwarded Tool Builder
23. PR-23 Deployment, Updates and Airgapped Support
24. PR-24 End-to-End MVP Plan

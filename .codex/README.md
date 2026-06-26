# Greentic Desktop — Detailed PR Pack

This package defines an implementation-ready roadmap for the Greentic Desktop project.

The design goal is:

```text
Prompt → Interactive desktop execution → User refinement → Recorded runner → Versioned approval → MCP tool publication → Reuse in Greentic flows, AWS WorkSpaces, or other MCP clients
```

The desktop automation runtime is intentionally generic. AWS WorkSpaces is one execution environment, not the architecture itself.

## Main Concepts

- **Desktop Runner Runtime**: Rust executable that hosts adapters, sessions, recording, replay and MCP.
- **Adapters**: Web, Windows UI, Java, terminal/mainframe, Office, vision fallback.
- **Runner Package**: Versioned, signed, Git-friendly recorded automation capability.
- **Prompt Builder**: Converts user prompts into executable runner drafts.
- **LLM Integration**: Planning and refinement use `greentic-llm` when available.
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
25. PR-25 Cross-Platform Desktop Platform Layer
26. PR-26 macOS Accessibility Adapter
27. PR-27 Linux Desktop Adapter: X11 First
28. PR-28 Linux Wayland Compatibility Layer
29. PR-29 Cross-Platform App Launcher and Window Manager
30. PR-30 Cross-Platform Input and Screenshot Backend
31. PR-31 Cross-Platform Recording Format Upgrade
32. PR-32 macOS/Linux CI and Test Desktop Harness
33. PR-33 greentic-llm Prompt-to-Runner Integration
34. PR-34 Record Command and Recording Session Lifecycle
35. PR-35 Embed Automate Hub Frontend Assets
36. PR-36 GUI Host and Default Browser Startup
37. PR-37 Local GUI API and Type Contract
38. PR-38 Home, Settings, Extensions, and Setup Integration
39. PR-39 Prompt-to-Runner GUI Wizard
40. PR-40 Recording GUI Wizard and Recorder Bridge
41. PR-41 Runner Management, Validation, and Publishing
42. PR-42 MCP Tools GUI and Server Lifecycle
43. PR-43 Approvals, Evidence, Activity, and Refinement UX
44. PR-44 Windows Click-to-Run Packaging and Release
45. PR-45 GUI Security, Localhost Boundaries, and Operational Hardening
46. PR-46 GUI Documentation and End-to-End User Flows
47. PR-47 Extension Package Format and OCI Artifact Layout
48. PR-48 greentic-distributor-client Integration for Extensions
49. PR-49 Local Extension Store, Install, Update and Remove
50. PR-50 Extension Store Index and Friendly Aliases
51. PR-51 Extension Build and Publish Pipeline to GHCR
52. PR-52 Extension Signing, Verification and Trust Policy
53. PR-53 Extension Manager API for Web UI
54. PR-54 Generic DesktopWorkflow Model and Adapter Compilation
55. PR-55 Higher-Level Runner Schema and Semantic Actions
56. PR-56 Recorder-Derived Inputs, Secrets, Outputs and Questions
57. PR-57 Capability-Router Planner
58. PR-58 Serde Runner Schema and JSON Schema Export
59. PR-59 Real Replay Dispatch, Assertions and Output Extraction
60. PR-60 Generic Web Adapter Semantics
61. PR-61 Generic MCP Published Runner Fixtures
62. PR-62 Store Index Generation from Extension Manifests
63. PR-63 Real Recorder Session Runtime and Event Stream
64. PR-64 Native Desktop Recording Backends
65. PR-65 Web, Terminal, and Remote Recording Backends
66. PR-66 Recording Normalisation, Locators, Redaction, and Evidence
67. PR-67 Recorder GUI for Real Capture, Evidence Review, and Repair
68. PR-68 Recorder End-to-End Fixtures and CI Harness
69. PR-69 Strict LLM Planning Contracts and Repair Loops
70. PR-70 Requirements Conversation and Clarification Wizard
71. PR-71 LLM Planner Orchestrator and Capability-Aware Draft Generation
72. PR-72 Prompt-Based Runner Update and Diff Apply
73. PR-73 LLM Evaluation Fixtures and Golden Prompt Tests
74. PR-74 LLM Planning UX, Traceability, and Controls

## Roadmap Alignment

- **GUI foundation:** PR-35 through PR-37 embed the Automate Hub, start it from `greentic-desktop`, and define the local JSON API.
- **GUI extension surface:** PR-38 consumes the remote extension capabilities from PR-47 through PR-53 in Settings > Extensions.
- **Remote extension backend:** PR-47 through PR-53 define package format, distributor resolution, local store, friendly index, GHCR publishing, signing/trust, and the web API.
- **Create and operate runners:** PR-39 through PR-43 connect prompt, recording, runners, MCP, evidence, approvals, and refinement to the same backend.
- **Shipping and hardening:** PR-44 through PR-46 cover Windows click-to-run, GUI security, and user documentation, including extension trust prompts and store workflows.
- **Generic automation foundation:** PR-54 through PR-62 remove app-specific assumptions from workflow authoring, planning, replay, MCP publication, web automation, and store metadata so Greentic Desktop can prompt, record, execute, and expose automations across desktop technologies.
- **Reliable LLM authoring and updates:** PR-69 through PR-74 make first-runner creation and prompt-based runner updates schema-constrained, question-driven, repairable, testable, and traceable.

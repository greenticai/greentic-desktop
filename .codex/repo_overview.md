# Repository Overview

## 1. High-Level Purpose

This repository is the early Rust workspace for the Greentic desktop runner. The intended product is a desktop automation runtime that can load adapters, manage sessions, record and replay runner packages, expose MCP tools, capture evidence, enforce security policy, and support reuse from Greentic flows or other MCP clients.

The current implemented state covers PR-01 through PR-34 foundations: core capability and runner-package policy types, a universal adapter SDK, capability validation and adapter selection, signed extension manifests, extension installation/listing/verification, sidecar launch metadata, Rust-side Playwright, Windows UI, Java accessibility, terminal/mainframe, vision fallback, macOS Accessibility, Linux X11, constrained Linux Wayland adapter models, a portable app/window management layer, cross-platform input/screenshot backend, upgraded cross-platform recording format, and macOS/Linux/Windows desktop CI harness modeling, cross-platform desktop detection and capability gating, session bootstrap profiles, recording engine and portable runner package model, prompt-to-runner planner, greentic-LLM request integration, runner draft schema validation, planner policy checks, interactive refinement loop, replay engine and validation model, evidence bundles and immutable audit storage, runner registry/versioning/signing, MCP tool publishing, security/secrets/policy enforcement, LTM/root-cause learning, AWS WorkSpaces integration models, workspace patch/test rollout flows, business-process automation, forwarded tool builder, deployment/update/airgapped support models, end-to-end MVP readiness/demo modeling, runtime configuration, session state, telemetry logging, a runtime host, CLI entrypoints for `greentic-desktop` and `gtc desktop`, CLI prompt planning and recording lifecycle commands, CI/release automation, and lightweight performance/concurrency checks.

## 2. Main Components and Functionality

- **Path:** `Cargo.toml`
  - **Role:** Root Cargo workspace manifest.
  - **Key functionality:** Defines the PR-01 through PR-34 workspace crates, shared package metadata, shared dependency versions, shared internal path dependencies, and the current workspace version `0.1.1`; member crates inherit package versions from `[workspace.package]` and dependency declarations from `[workspace.dependencies]`.
  - **Key dependencies / integration points:** Used by all cargo commands, `ci/local_check.sh`, and GitHub Actions. `greentic-desktop` is the installable CLI package; its direct/transitive runtime dependency crates are publishable so crates.io and cargo-binstall can resolve it.

- **Path:** `crates/greentic-desktop-core`
  - **Role:** Core types and validation utilities.
  - **Key functionality:** Defines `Capability`, `RiskLevel` including low/medium/high/critical, `CapabilityError`, `RunnerPackageRef`, and package policy decisions; normalizes capabilities; rejects duplicate or malformed capability declarations; evaluates whether unsigned published runner packages are allowed; provides `checksum_workload` for fast performance/concurrency checks.
  - **Key dependencies / integration points:** Used by the runtime crate, unit tests, integration performance tests, Criterion benchmarks, and crates.io dry-run packaging.

- **Path:** `crates/greentic-desktop-config`
  - **Role:** Runtime configuration model.
  - **Key functionality:** Provides default runner, security, MCP, and evidence configuration matching PR-01/PR-03 values; supports `GREENTIC_DESKTOP_HOME`; renders the current config as TOML for `gtc desktop config show`; includes `require_signed_extensions`.
  - **Key dependencies / integration points:** Used by runtime and CLI crates.

- **Path:** `crates/greentic-desktop-adapter`
  - **Role:** Universal adapter SDK and capability model.
  - **Key functionality:** Defines the `DesktopAdapter` trait, `AdapterCapabilities`, observe/execute/validate/record contexts and results, generic `RunnerStep`, `Assertion`, `RecordedEvent`, locator strategies with preferred/fallback/visual fallback targets, capability validation, best-adapter selection, and a `StaticAdapter` test/helper implementation.
  - **Key dependencies / integration points:** Reuses `greentic-desktop-core::Capability` and `RiskLevel`. Used by the runtime adapter registry to validate required capabilities before execution and select an installed adapter.

- **Path:** `crates/greentic-desktop-extension`
  - **Role:** Extension manager and sidecar runtime metadata.
  - **Key functionality:** Defines extension manifests, native/sidecar runtime types, signed-extension verification, manifest rendering/parsing, installed extension listing, built-in registry entries for Playwright and TN3270 adapters, adapter-capability conversion, and sidecar process metadata preparation.
  - **Key dependencies / integration points:** Reuses `greentic-desktop-adapter::AdapterCapabilities`. Used by runtime extension install/list/verify/start-sidecar methods and CLI extension commands.

- **Path:** `crates/greentic-desktop-web`
  - **Role:** Rust-side Playwright web adapter model.
  - **Key functionality:** Exposes Playwright web capabilities (`web.goto`, `web.click`, `web.fill`, `web.select`, `web.wait_for_text`, `web.extract_text`, `web.extract_regex`, `web.screenshot`, `web.assert_visible`, `web.assert_url`, `web.download_file`); implements selector priority for `data-testid`, role/name, label, text, CSS, XPath, and visual fallback; records human interactions with secret redaction; simulates open/fill/submit/extract/replay behavior for deterministic tests.
  - **Key dependencies / integration points:** Implements the PR-02 `DesktopAdapter` trait. The built-in Playwright extension manifest advertises the same capability set for sidecar installation.

- **Path:** `crates/greentic-desktop-windows`
  - **Role:** Windows UI Automation adapter model.
  - **Key functionality:** Exposes Windows capabilities (`windows.open_app`, `windows.find_window`, `windows.find_element`, `windows.click_element`, `windows.type_text`, `windows.read_text`, `windows.read_window_tree`, `windows.assert_visible`, `windows.screenshot`, `windows.close_app`); models automation ID/name/control type/class name/relative position/visual fallback locators; records control interactions; simulates app open, control lookup, form fill, error dialog detection, and replay after reboot.
  - **Key dependencies / integration points:** Implements the PR-02 `DesktopAdapter` trait. A built-in `greentic.desktop.windows-ui` extension manifest advertises the same capability set.

- **Path:** `crates/greentic-desktop-java`
  - **Role:** Java desktop accessibility adapter model.
  - **Key functionality:** Exposes Java capabilities (`java.find_window`, `java.find_component`, `java.click_component`, `java.type_text`, `java.read_text`, `java.assert_visible`, `java.capture_tree`); models component name, role, text, keyboard shortcut, and visual fallback locators; supports Access Bridge enabled and fallback modes; records component interactions; simulates form replay and visibility assertions.
  - **Key dependencies / integration points:** Implements the PR-02 `DesktopAdapter` trait. A built-in `greentic.desktop.java-accessibility` sidecar extension manifest advertises the same capability set.

- **Path:** `crates/greentic-desktop-terminal`
  - **Role:** Terminal and mainframe adapter model.
  - **Key functionality:** Exposes terminal capabilities (`terminal.connect`, `terminal.disconnect`, `terminal.read_screen`, `terminal.send_keys`, `terminal.send_text`, `terminal.type_text`, `terminal.wait_for_screen`, `terminal.assert_text`, `terminal.extract_field`, `terminal.capture_screen`); models VT/TN/SSH/serial protocols; records screen buffers; replays login and menu navigation; asserts expected text; extracts values by row/column or text anchor.
  - **Key dependencies / integration points:** Implements the PR-02 `DesktopAdapter` trait. The built-in TN3270 extension manifest advertises the same capability set.

- **Path:** `crates/greentic-desktop-vision`
  - **Role:** Vision and screenshot fallback adapter model.
  - **Key functionality:** Exposes vision capabilities (`vision.screenshot`, `vision.find_text`, `vision.find_button`, `vision.click_region`, `vision.compare_baseline`, `vision.assert_visual`, `vision.extract_text`); models regions, confidence scores, baseline comparisons, visual evidence records, text matching, visual clicks, and assertion explanations.
  - **Key dependencies / integration points:** Implements the PR-02 `DesktopAdapter` trait. A built-in `greentic.desktop.vision` sidecar extension manifest advertises the same capability set.

- **Path:** `crates/greentic-desktop-recorder`
  - **Role:** Recording engine and portable runner package model.
  - **Key functionality:** Captures human demonstration events, prompt-generated steps, hybrid recordings, screenshots references, normalized summaries, redacted sensitive values, merged prompt/recorded steps, deterministic runner YAML output, recording session states/manifests, append-only raw JSONL events, start/pause/resume/stop/cancel/status/list lifecycle operations, normalisation/finalisation helpers, portable platform support declarations, per-platform preferred adapters, OS-specific app launch values and locators, portable logical steps, runtime replay-plan selection, unsupported-platform preflight failures, and platform-path evidence URIs.
  - **Key dependencies / integration points:** Consumes `RecordedEvent`, `LocatorTarget`, and `RunnerStep` from the adapter SDK. Portable replay plans are intended to feed platform/windowing/input adapters.

- **Path:** `crates/greentic-desktop-llm`
  - **Role:** Thin greentic-LLM prompt planning adapter.
  - **Key functionality:** Defines the structured `desktop.prompt_to_runner` LLM request envelope, model policy, planning context, response/error types, mock static client, and deterministic heuristic client used by local CLI/tests when no external LLM service is configured.
  - **Key dependencies / integration points:** Used by the planner crate as the LLM abstraction boundary so future real `greentic-llm` wiring can replace the heuristic client without changing planner validation.

- **Path:** `crates/greentic-desktop-runner-schema`
  - **Role:** Runner draft schema validation.
  - **Key functionality:** Parses constrained JSON runner drafts from LLM output, validates required fields, risk level, namespaced capabilities, step shape, and clarification-only drafts, converts validated drafts into recorder `RunnerPackage` values, and emits structured `planner.*` diagnostics for invalid output.
  - **Key dependencies / integration points:** Uses adapter runner steps, core risk levels, and recorder runner packages. Used by the planner before saving any LLM-produced draft.

- **Path:** `crates/greentic-desktop-policy`
  - **Role:** Prompt-planning policy checks.
  - **Key functionality:** Applies planner policy to validated runner drafts, denying critical drafts by default, optionally blocking high-risk drafts, and requiring inputs for destructive/submitting actions.
  - **Key dependencies / integration points:** Uses core risk levels and runner-schema draft documents. Used by the planner after schema validation and before file writes.

- **Path:** `crates/greentic-desktop-planner`
  - **Role:** Prompt-to-runner planner.
  - **Key functionality:** Produces draft runner packages from natural-language prompts; builds greentic-LLM request context, accepts structured LLM draft JSON, validates schema and required capabilities, applies planner policy, writes draft runner YAML, supports dry-run planning, and still provides the earlier deterministic prompt/context planner for modeled flows.
  - **Key dependencies / integration points:** Uses adapter capabilities, core risk levels, the LLM adapter crate, runner-schema validation, planner policy, session profiles, and recorder runner packages.

- **Path:** `crates/greentic-desktop-refinement`
  - **Role:** Interactive refinement loop.
  - **Key functionality:** Captures runtime context shown to users, parses natural-language corrections, previews scoped runner diffs, and applies changes to failed steps without rewriting unrelated steps.
  - **Key dependencies / integration points:** Updates recorder `RunnerPackage` steps and adapter locators.

- **Path:** `crates/greentic-desktop-replay`
  - **Role:** Replay engine and validation model.
  - **Key functionality:** Validates runner package requirements against installed adapter capabilities; resolves declared inputs and secrets; simulates step execution, assertions, retry policy, failure handling, output extraction, structured step traces, and evidence bundle references.
  - **Key dependencies / integration points:** Consumes recorder `RunnerPackage`/`RunnerOutputSpec`, adapter capabilities, and evidence bundle types. Future runtime execution can replace the deterministic simulator behind the same request/outcome model.

- **Path:** `crates/greentic-desktop-evidence`
  - **Role:** Evidence store and audit bundle model.
  - **Key functionality:** Defines audit-quality evidence bundles with run metadata, redacted input hashing, outputs, typed artifacts, screenshots, tool traces, immutable in-memory storage, JSON rendering, and MCP-result evidence references.
  - **Key dependencies / integration points:** Used by the replay crate so every replay outcome carries an evidence bundle and reference. Future object-store/control-plane backends can implement the same storage semantics.

- **Path:** `crates/greentic-desktop-registry`
  - **Role:** Runner registry, versioning, and signing model.
  - **Key functionality:** Defines runner lifecycle states, dev/staging/prod stages, exact/channel version selectors, tenant/team/private scoping, reviewable manifest rendering, deterministic scaffold signing, signature verification, tamper detection, promotion, rollback-ready resolution, and immutable registry entries.
  - **Key dependencies / integration points:** Used by runtime verification so tampered signed runner manifests are refused before load. Future registry backends can use the same signed manifest and lifecycle model.

- **Path:** `crates/greentic-desktop-mcp`
  - **Role:** MCP server and tool publishing model.
  - **Key functionality:** Publishes approved runner packages as stable MCP tool descriptors, renders `tools/list`, executes `tools/call` through replay, checks tool permissions, security policy, required inputs/secrets, human approval flags, and rate limits, returns structured outputs/failures with evidence references, and produces AWS forwarded tool names.
  - **Key dependencies / integration points:** Uses recorder packages, adapter capabilities, replay outcomes, session profiles, security policy decisions, and core risk levels. The runtime MCP HTTP shim now returns a generated `tools/list` response for the example published runner.

- **Path:** `crates/greentic-desktop-security`
  - **Role:** Security, secrets, and policy enforcement model.
  - **Key functionality:** Defines permission policy, approval policy, environment allow-lists, low/medium/high/critical risk enforcement, published-runner signature requirements, dangerous action blocking, `SecretsManager` reference resolution, and text redaction for logs/LTM.
  - **Key dependencies / integration points:** Uses core `RiskLevel` and registry signed manifest lifecycle data. The MCP crate enforces policy before replaying a tool call.

- **Path:** `crates/greentic-desktop-ltm`
  - **Role:** Long-term memory and root-cause learning model.
  - **Key functionality:** Stores run outcomes from evidence bundles, app/image/patch versions, inputs hash, outputs, screenshots, failures, human corrections, root causes, fixes, approval decisions, and final outcomes; retrieves similar failures; emits planner context; generates RCA summaries.
  - **Key dependencies / integration points:** Consumes evidence bundles. Future planner and control-plane backends can use the same memory cases and similarity/RCA helpers.

- **Path:** `crates/greentic-desktop-workspaces`
  - **Role:** AWS WorkSpaces integration model.
  - **Key functionality:** Models installed-inside-Workspace and AWS-managed-MCP-forwarding patterns; plans runtime installation into golden images; pulls approved signed runners from the registry; exposes workspace runner tools through MCP; calls forwarded runners and returns evidence references.
  - **Key dependencies / integration points:** Uses registry version/signature verification, MCP published tools, and session workspace attach profiles without making the core architecture AWS-specific.

- **Path:** `crates/greentic-desktop-rollout`
  - **Role:** Workspace patch, test, rollout, and rollback flow model.
  - **Key functionality:** Models canary rings, patch details, wait windows, multi-runner validation, pass/fail evidence reports, failed assertions, rollout decisions, and rollback/pause/ticket/notify actions.
  - **Key dependencies / integration points:** Uses WorkSpaces forwarded runner calls and MCP results so validation evidence is attached to rollout decisions.

- **Path:** `crates/greentic-desktop-business`
  - **Role:** Business-process automation with desktop runners.
  - **Key functionality:** Models production flows such as customer onboarding, request data collection, required input validation, sequential runner calls, downstream output binding, production human approval, confirmation steps, and compliance evidence URI collection.
  - **Key dependencies / integration points:** Uses MCP published runner tools and call results so recorded desktop runners can operate as production business tools.

- **Path:** `crates/greentic-desktop-forwarded`
  - **Role:** Forwarded MCP tool builder.
  - **Key functionality:** Converts signed published runner manifests and runner packages into local or AWS-forwarded MCP tool metadata, input/output schemas, risk/evidence/permission metadata, `PublishedRunnerTool` registrations, and callable tool results with evidence references.
  - **Key dependencies / integration points:** Uses registry signing/lifecycle data, recorder packages, adapter capabilities, MCP server state, security policies, and session profiles.

- **Path:** `crates/greentic-desktop-deployment`
  - **Role:** Deployment, updates, and airgapped support model.
  - **Key functionality:** Defines connected and airgapped update packages for runtime, extension, runner, policy, and revocation-list payloads; signs update manifests; verifies signatures, checksums, source mode, tenant scope, revocations, runtime compatibility, and extension/policy dependencies; produces install plans, rollback actions, and audit log entries.
  - **Key dependencies / integration points:** Uses extension manifests and registry signed runner manifests/signing keys so approved runner packages and adapter extensions can be pulled from online registries or imported from local bundles under the same verification path.

- **Path:** `crates/greentic-desktop-mvp`
  - **Role:** End-to-end MVP readiness and demo model.
  - **Key functionality:** Defines MVP readiness requirements across runtime, adapters, builder, MCP, evidence, and workspace-worker areas; runs a deterministic CRM customer creation demo from prompt draft through interactive correction, signed publication as `crm.create_customer`, MCP JSON invocation with evidence, and Workspace patch validation using `crm.validate_app`; reports PR-24 success criteria.
  - **Key dependencies / integration points:** Uses adapter capabilities, planner, refinement, registry signing, forwarded MCP tool building, WorkSpaces validation tools, and rollout evidence reports.

- **Path:** `crates/greentic-desktop-platform`
  - **Role:** Cross-platform desktop platform layer.
  - **Key functionality:** Detects Windows/macOS/Linux, models display servers such as X11, Wayland, Quartz, desktop, or RDP, lists platform capabilities (`platform.detect`, permissions checks/explanations, app/window/screenshot/input operations), explains platform permissions, maps runner package steps to platform capability requirements, and rejects runner packages when the current desktop cannot support required capabilities.
  - **Key dependencies / integration points:** Uses recorder runner packages and adapter step capability names so runtime/replay layers can gate packages before execution. Future macOS/Linux adapters can use this as their shared capability and permission contract.

- **Path:** `crates/greentic-desktop-macos`
  - **Role:** macOS Accessibility adapter model.
  - **Key functionality:** Exposes native macOS capabilities (`macos.find_app`, `macos.find_window`, `macos.read_window_tree`, `macos.find_element`, `macos.click_element`, `macos.type_text`, `macos.read_text`, `macos.assert_visible`, `macos.screenshot`, `macos.activate_app`, `macos.close_app`); models AX identifier/title/role/value locators plus visual fallback; provides first-run Accessibility, Screen Recording, and Input Monitoring diagnostics; simulates app activation, AX tree inspection, button clicks, text entry, assertions, screenshots, and permission failures.
  - **Key dependencies / integration points:** Implements the adapter SDK and uses the shared platform permission model. The built-in extension registry exposes `greentic.desktop.macos.ax` as a signed native adapter.

- **Path:** `crates/greentic-desktop-linux`
  - **Role:** Linux desktop adapter model.
  - **Key functionality:** Exposes X11-first Linux capabilities (`linux.find_window`, `linux.read_window_tree`, `linux.find_element`, `linux.click_element`, `linux.type_text`, `linux.read_text`, `linux.assert_visible`, `linux.screenshot`, `linux.activate_window`, `linux.close_window`) plus constrained Wayland capabilities (`linux.wayland.detect`, `linux.wayland.portal_screenshot`, `linux.wayland.accessibility_tree`, `linux.wayland.assert_visible`, `linux.wayland.safe_keyboard_shortcut`); detects X11 versus Wayland; models AT-SPI-style accessible name/role plus window/class fallback and visual fallback; lists X11 windows; simulates GTK/Qt control interaction, XTest keyboard/mouse fallback for X11, xdg-desktop-portal screenshots for Wayland, AT-SPI automation where available, and explicit manual-approval/unsupported failures for restricted Wayland operations.
  - **Key dependencies / integration points:** Implements the adapter SDK and uses the shared platform model. The built-in extension registry exposes `greentic.desktop.linux.x11` and `greentic.desktop.linux.wayland` as signed native adapters.

- **Path:** `crates/greentic-desktop-windowing`
  - **Role:** Cross-platform app launcher and window manager model.
  - **Key functionality:** Defines portable `desktop.open_app`, `desktop.find_window`, `desktop.activate_window`, `desktop.list_windows`, `desktop.close_window`, and `desktop.window_screenshot` capabilities; models launch requirements for Windows executables/Start menu/PowerShell, macOS bundle IDs/app names/app paths, and Linux `.desktop` entries/executables/Flatpak/Snap/AppImage; lists, finds, activates, closes, screenshots, and restores target windows before replay; prepends portable restore steps to runner packages.
  - **Key dependencies / integration points:** Uses platform information, adapter runner steps, and recorder runner packages. Provides mapping from generic `desktop.*` capabilities back to existing Windows/macOS/Linux adapter capabilities for compatibility.

- **Path:** `crates/greentic-desktop-io`
  - **Role:** Cross-platform input and screenshot backend.
  - **Key functionality:** Defines `input.move_mouse`, `input.click`, `input.double_click`, `input.drag`, `input.type_text`, `input.hotkey`, `screen.screenshot`, `screen.region_screenshot`, `screen.locate_text`, and `screen.locate_image` primitives; routes to Windows UIA/Win32, macOS CoreGraphics/Accessibility, Linux X11/XTest, or Wayland portal-limited backends; enforces keyboard/mouse/screenshot permissions; produces consistent screenshot evidence artifacts and locate confidence/regions; surfaces Wayland global-input and portal-screenshot limitations as capability failures.
  - **Key dependencies / integration points:** Uses the shared platform model and evidence store types so adapters and vision fallback can capture screenshots consistently regardless of OS.

- **Path:** `crates/greentic-desktop-test-harness`
  - **Role:** macOS/Linux/Windows desktop test harness model.
  - **Key functionality:** Defines harness jobs for macOS GitHub Actions unit tests, manual permission-gated macOS tests, Ubuntu X11 virtual display tests, Ubuntu Wayland graceful-degradation tests, and Windows unit tests; declares sample GTK, Qt, SwiftUI/AppKit, and Java Swing desktop targets; verifies X11 detection, Wayland limitations, macOS permission diagnostics, and CI matrix coverage.
  - **Key dependencies / integration points:** Uses macOS, Linux, and platform crates. `.github/workflows/desktop-harness.yml` runs the modeled matrix and the sample target descriptors live in `examples/desktop-targets/`.

- **Path:** `crates/greentic-desktop-session`
  - **Role:** Desktop session lifecycle primitives.
  - **Key functionality:** Defines `DesktopSession` and `SessionState` with create, attach, and close transitions; defines `SessionProfile`, bootstrap actions, teardown actions, browser kinds, and bootstrap planning for local web apps, native apps, terminal hosts, and workspace attach mode.
  - **Key dependencies / integration points:** Used by the runtime host as the initial session abstraction.

- **Path:** `crates/greentic-desktop-telemetry`
  - **Role:** In-memory telemetry event logging.
  - **Key functionality:** Defines `TelemetryEvent` and cloneable `TelemetryLog`; records tool calls, session starts, and runner load events with timestamps.
  - **Key dependencies / integration points:** Used by the runtime host to satisfy PR-01 logging expectations.

- **Path:** `crates/greentic-desktop-runtime`
  - **Role:** Runtime host scaffold.
  - **Key functionality:** Loads default config, reports runtime info, initializes home/evidence/extension directories, starts sessions, discovers installed extensions and local `.gtpack` runners, exposes installed adapter capabilities, validates required capabilities, selects a supporting adapter, installs signed built-in extensions, verifies installed extensions, prepares sidecar metadata, enforces unsigned-runner and unsigned-extension policy, verifies signed registry runner manifests, refuses tampered runner manifests, logs runtime actions, and serves HTTP JSON health/tools-list responses for `mcp serve`.
  - **Key dependencies / integration points:** Depends on adapter, core, config, extension, MCP, registry, session, and telemetry crates. Used by the CLI crate.

- **Path:** `crates/greentic-desktop-cli`
  - **Role:** Installable command-line package.
  - **Key functionality:** Publishes as the `greentic-desktop` crate and provides `greentic-desktop` and `gtc` binaries. Implemented commands are `info`, `init`, `config show`, `extension install ID`, `extension list`, `extension update`, `extension verify [ID]`, `extension sidecar ID`, `runner list`, `runner plan (--prompt TEXT|--prompt-file PATH) [--profile ID] [--context PATH] [--dry-run] [--out PATH]`, `record start/pause/resume/stop/cancel/status/list/normalise/finalise/mark-input/mark-secret/mark-output/add-assertion/note`, and `mcp serve [--bind ADDR]`; the `gtc` binary requires the `desktop` prefix.
  - **Key dependencies / integration points:** Calls runtime/config APIs, includes cargo-binstall metadata for Linux/macOS/Windows x64 and ARM release archives, and is intended to install through `cargo binstall greentic-desktop`. Verified manually with `greentic-desktop info` and `gtc desktop config show`.

- **Path:** `crates/greentic-desktop-core/tests/perf_scaling.rs`
  - **Role:** Lightweight integration tests for concurrency and timeout protection.
  - **Key functionality:** Runs the deterministic workload across 1, 4, and 8 threads; verifies worker result consistency; checks an 8-thread workload completes within 5 seconds; checks scaling does not degrade beyond a generous PR-CI threshold.
  - **Key dependencies / integration points:** Runs under `cargo test --all-features` and therefore under `ci/local_check.sh` and CI.

- **Path:** `crates/greentic-desktop-core/benches/perf.rs`
  - **Role:** Criterion benchmark harness.
  - **Key functionality:** Benchmarks `checksum_workload(10_000)` and `normalize_capabilities`.
  - **Key dependencies / integration points:** Compiled by `cargo clippy --all-targets`; can be run manually with `cargo bench -p greentic-desktop-core`.

- **Path:** `ci/local_check.sh`
  - **Role:** Single local developer validation entrypoint.
  - **Key functionality:** Runs formatting, Clippy with denied warnings, tests, build, docs, `cargo package --no-verify`, verified `cargo package`, and `cargo publish --dry-run` for publishable crates. Publish ordering includes the prompt-planning support crates needed by the CLI.
  - **Key dependencies / integration points:** Passed after PR-01 implementation. Used directly by developers and by workflow jobs.

- **Path:** `.github/workflows/ci.yml`
  - **Role:** Pull request and main-branch CI workflow.
  - **Key functionality:** Runs parallel lint, test, and package dry-run jobs with concurrency cancellation for redundant runs.
  - **Key dependencies / integration points:** Calls `.github/workflows/_reusable_rust.yml`.

- **Path:** `.github/workflows/publish.yml`
  - **Role:** crates.io release workflow.
  - **Key functionality:** Triggers from `workflow_dispatch` or `v*` tags; verifies tag `v<version>` matches `greentic-desktop`; runs `ci/local_check.sh`; on tag releases builds Linux/macOS/Windows x64 and ARM binary archives for cargo-binstall; verifies archive paths before upload; performs final dry-runs and idempotent real publishes for the CLI crate and its publishable dependency chain, waiting for each crates.io version to become visible before publishing downstream dependents.
  - **Key dependencies / integration points:** Requires `CARGO_REGISTRY_TOKEN` for crates.io and GitHub `contents: write` for release asset uploads. GHCR publishing is not enabled.

- **Path:** `README.md`
  - **Role:** Public non-technical overview.
  - **Key functionality:** Explains Greentic Desktop at a high level for non-technical users, describes the runner lifecycle, lists current CLI entry points including prompt planning and recording, links to detailed feature docs, and clearly distinguishes the implemented CLI/runtime surface from the broader modeled product workflows.

- **Path:** `docs/`
  - **Role:** Detailed feature documentation.
  - **Key functionality:** Provides in-depth docs for getting started, runners, adapters and desktop support, per-adapter usage guides for every built-in extension, recording/refinement, MCP tools, AWS WorkSpaces MCP forwarding, security/secrets/approvals, evidence, business workflows, deployment/rollout, CLI usage, and developer notes.

- **Path:** `.codex/README.md` and `.codex/done/PR-*.md`
  - **Role:** Completed implementation roadmap for the desktop runner.
  - **Key functionality:** Documents the PR sequence and completed implementation briefs through PR-34; indexes active planning PRs for the Automate Hub GUI and remote extension distribution/store work through PR-53.
  - **Key dependencies / integration points:** Used as historical implementation context and implementation-ready planning context; active code lives in workspace crates.

- **Path:** `.codex/global_rules.md` and `.codex/repo_overview_task.md`
  - **Role:** Codex working rules and overview maintenance routine.
  - **Key functionality:** Require maintaining this overview around PR-style work, running `ci/local_check.sh`, and preferring shared Greentic crates for reusable types and behavior.

## 3. Work In Progress, TODOs, and Stubs

- **Location:** `.codex/done/PR-01-*.md` through `.codex/done/PR-34-*.md`
  - **Status:** Complete
  - **Short description:** The full PR roadmap has been implemented through greentic-LLM prompt planning integration and CLI recording session lifecycle.

- **Location:** `crates/greentic-desktop-runtime::serve_mcp`
  - **Status:** Complete
  - **Short description:** Implements the current scoped runtime MCP surface: `mcp serve` binds successfully, responds to health checks, and returns a generated JSON `tools/list` response for the example published runner.

- **Location:** `crates/greentic-desktop-adapter::DesktopAdapter`
  - **Status:** Complete
  - **Short description:** Implements the PR-02 adapter contract with synchronous observe/execute/validate/record operations, capability validation, adapter selection, locators, assertions, and helper implementations used across the workspace.

- **Location:** `crates/greentic-desktop-extension::SidecarProcess`
  - **Status:** Complete
  - **Short description:** Implements the current sidecar metadata lifecycle: signed manifest verification, installation, listing, parsing, capability extraction, and command/argument preparation for installed sidecar adapters.

- **Location:** `crates/greentic-desktop-web`
  - **Status:** Complete
  - **Short description:** Implements the Rust-side Playwright adapter model, including web capability exposure, selector priority, recording redaction, deterministic replay semantics, and matching built-in extension metadata.

- **Location:** `crates/greentic-desktop-windows`
  - **Status:** Complete
  - **Short description:** Implements the portable Windows UI Automation adapter model, including capability exposure, UIA-style locators, form replay, error-dialog detection, screenshot capability modeling, and reboot replay behavior.

- **Location:** `crates/greentic-desktop-java`
  - **Status:** Complete
  - **Short description:** Implements the Java accessibility adapter model, including Access Bridge-style locators, fallback modes, component interaction recording, form replay, visibility assertions, and sidecar extension metadata.

- **Location:** `crates/greentic-desktop-terminal`
  - **Status:** Complete
  - **Short description:** Implements the terminal/mainframe adapter model, including VT/TN/SSH/serial protocol modeling, screen-buffer recording, login/menu replay, text assertions, field extraction, and capture-screen capability metadata.

- **Location:** `crates/greentic-desktop-vision`
  - **Status:** Complete
  - **Short description:** Implements the deterministic vision fallback adapter model, including screenshot, text/button lookup, visual click regions, baseline comparison, visual assertions, extracted text, and visual evidence records.

- **Location:** `README.md`, `.github/workflows/publish.yml`
  - **Status:** Complete
  - **Short description:** cargo-binstall metadata and release artifact publishing are configured for `greentic-desktop` on Linux, macOS 15, and Windows across x64 and ARM. GHCR publishing remains intentionally omitted.

- **Location:** Repository-wide marker search
  - **Status:** No inline TODO/stub markers found
  - **Short description:** A search for `TODO`, `FIXME`, `XXX`, `HACK`, `TEMP`, `BROKEN`, `unimplemented`, `todo!`, and `NotImplemented` outside `target/` returned no matches.

## 4. Broken, Failing, or Conflicting Areas

- **Location:** `ci/local_check.sh`
  - **Evidence:** `bash ci/local_check.sh` passed after PR-33/PR-34 implementation.
  - **Likely cause / nature of issue:** No current local check failure. The successful run covered formatting, Clippy with denied warnings, tests, build, docs, package verification, and `cargo publish --dry-run` for locally packageable crates including the new `greentic-desktop-llm` crate.

- **Location:** Product implementation versus `.codex/PR-*.md` roadmap
  - **Evidence:** PR-01 through PR-34 now have corresponding workspace implementations and their tracking documents are kept under `.codex/done/`.
  - **Likely cause / nature of issue:** No remaining PR roadmap mismatch is known. Future work should focus on replacing deterministic models with real sidecar/native integrations and a full MCP protocol server where needed.

## 5. Notes for Future Work

- Use `crates/greentic-desktop-test-harness` and `.github/workflows/desktop-harness.yml` as the starting point for real desktop integration harness hardening.
- Expand the scoped MCP HTTP response into a full protocol server if the product needs a broader MCP transport surface than the current modeled endpoint.
- Decide whether i18n support is required before adding more user-facing CLI output.
- Decide explicitly whether future releases need GHCR publishing; the current automation covers crates.io plus GitHub Release binaries for cargo-binstall.
- Continue refreshing this file before and after PR-style changes, and keep `ci/local_check.sh` as the authoritative local validation command.

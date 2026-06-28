# Greentic Desktop

Greentic Desktop helps teams turn repeatable desktop work into safe, reusable automation.

It is designed for situations where important work still happens inside desktop apps, web portals, terminal systems, Java applications, remote desktops, or legacy business tools. A Greentic Desktop "runner" describes that work once, checks that it is allowed, replays it on a compatible desktop, and keeps evidence of what happened.

## What You Can Use It For

- Open an application or web page and complete a known task.
- Turn a demonstrated workflow into a reusable runner.
- Publish approved runners so an assistant or MCP client can call them.
- Run desktop tasks with inputs, secrets, approvals, and audit evidence.
- Support Windows, macOS, Linux, web, terminal, Java, and screenshot-based fallback paths.
- Validate desktop environments before rolling out patches or updates.

Greentic Desktop is not a general-purpose screen macro recorder. It is a controlled runner system: every runner declares what it needs, what it can do, which adapters it uses, and what evidence should be kept.

## How It Works

1. **Set up the desktop runtime.** The runtime creates a local Greentic Desktop home folder for runners, extensions, configuration, and evidence.
2. **Install adapters.** Adapters let Greentic Desktop work with web apps, native windows, Java apps, terminals, screenshots, and platform-specific desktop APIs.
3. **Create a runner.** A runner can come from a prompt, a human demonstration, or a mix of both.
4. **Review and refine it.** Corrections can update individual steps without requiring a user to hand-edit the runner package.
5. **Approve and publish it.** Published runners are signed, scoped to a tenant or team, and can be exposed as MCP tools.
6. **Run it safely.** Each call checks inputs, secrets, permissions, approvals, environment policy, and rate limits.
7. **Keep evidence.** Runs produce evidence references for audit, troubleshooting, and rollout decisions.

## Your First Automation

The usual path is:

1. **Record or describe the task once.** Start from a clear, repeatable task such as "open a resource table, ask for `resource_name`, `name`, and `email`, append a row, save, and return `saved_status`." Greentic Desktop observes or plans the app target, inputs, commands, outputs, assertions, and evidence checks.
2. **Turn it into a runner.** The recording or prompt is normalized into a runner package. The runner keeps stable targets, typed inputs, required secrets, expected outputs, output extractors, assertions, and evidence rules.
3. **Review and approve the runner.** A person checks the runner before it is used by others. Published runners are expected to be signed so production systems do not run unapproved desktop automation.
4. **Expose it as an MCP tool forwarder.** The approved runner is converted into a tool with a stable name, input schema, output schema, permissions, approval policy, and evidence settings. A local or forwarded MCP client can then call that tool instead of driving the desktop directly.
5. **Use it from an assistant or workflow.** The caller supplies the required inputs, Greentic Desktop replays the desktop task, and the response includes structured outputs plus an evidence reference.

The current CLI can initialize the runtime, install adapter manifests, plan draft runners from prompts, manage recording sessions, discover local `.gtpack` runner packages, and serve the MCP endpoint. Approval, registry publish, and forwarded-tool registration are still represented by product models and tests while their production command surface is being added. See [Getting Started](docs/getting-started.md), [Recording and Refinement](docs/recording-and-refinement.md), [Runners](docs/runners.md), and [MCP Tools](docs/mcp-tools.md) for the detailed workflow.

## Open Greentic Desktop

Start the GUI-first experience with:

```bash
greentic-desktop
```

Automate Hub opens in the browser. From there you can complete setup, create from prompt, record a task, test and save runners, publish approved runners as MCP tools, and copy MCP client configuration for AI workers. See [Automate Hub GUI](docs/gui.md) for the full browser workflow.

## Install Greentic Desktop

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.sh | sh
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.ps1 | iex
```

Then start Automate Hub:

```bash
greentic-desktop
```

Developer install with Rust remains available:

```bash
cargo binstall greentic-desktop
```

Manual release archive installation is also supported. See [One-Line Install](docs/install-one-line.md) and [Release And Installation](docs/release.md).

## Command-Line Entry Points

For Rust developers, install the CLI with cargo-binstall:

```bash
cargo binstall greentic-desktop
```

Windows users can also download the `x86_64-pc-windows-msvc` or
`aarch64-pc-windows-msvc` zip from a GitHub release, extract it, and
double-click `greentic-desktop.exe` to open Automate Hub. Public
`cargo binstall` requires public crates.io metadata and publicly accessible
release assets; private GitHub releases need an authenticated distribution path.
See [Release And Installation](docs/release.md).

From this repository, `greentic-desktop` with no arguments starts the local Automate Hub GUI, opens the default browser, and serves the embedded frontend from a loopback address. The current CLI can also show runtime information, initialize local storage, manage built-in extensions, plan draft runners, manage recording sessions, list local runner packages, and serve a small MCP endpoint.

```bash
greentic-desktop
greentic-desktop gui --no-open --bind 127.0.0.1:0
greentic-desktop info
greentic-desktop init
greentic-desktop extension install greentic.desktop.playwright
greentic-desktop extension list
greentic-desktop runner list
greentic-desktop runner plan --prompt "Open a resource table, ask for resource_name, name, and email, append a row, save, and return saved_status" --dry-run
greentic-desktop record start --name generic.resource_append --profile local-web --adapter greentic.desktop.playwright --out ./recordings/generic.resource_append
greentic-desktop mcp serve
```

The same commands are also available through the `gtc desktop` form:

```bash
gtc desktop info
```

## Detailed Documentation

- [Getting Started](docs/getting-started.md)
- [One-Line Install](docs/install-one-line.md)
- [Automate Hub GUI](docs/gui.md)
- [Runners](docs/runners.md)
- [Adapters and Supported Desktops](docs/adapters.md)
  - [Playwright Web Adapter](docs/adapters/playwright-web.md)
  - [Terminal TN3270 Adapter](docs/adapters/terminal-tn3270.md)
  - [Windows UI Adapter](docs/adapters/windows-ui.md)
  - [Java Accessibility Adapter](docs/adapters/java-accessibility.md)
  - [Vision Adapter](docs/adapters/vision.md)
  - [macOS Accessibility Adapter](docs/adapters/macos-accessibility.md)
  - [Linux X11 Adapter](docs/adapters/linux-x11.md)
  - [Linux Wayland Adapter](docs/adapters/linux-wayland.md)
- [Recording and Refinement](docs/recording-and-refinement.md)
- [Recording Runbooks](docs/recording-runbooks.md)
- [MCP Tools](docs/mcp-tools.md)
- [Security, Approvals, and Secrets](docs/security.md)
- [Evidence and Audit Trail](docs/evidence.md)
- [Business Workflows](docs/business-workflows.md)
- [Deployment and Rollout](docs/deployment-and-rollout.md)
- [Release And Installation](docs/release.md)
- [Extension Store](docs/extension-store.md)
- [Extension Package Format](docs/extension-package-format.md)
- [Extension GHCR Publish Pipeline](docs/extension-ghcr-pipeline.md)
- [End-To-End Smoke Checklist](docs/e2e-smoke.md)
- [AWS WorkSpaces MCP Forwarding](docs/aws-workspaces-mcp.md)
- [CLI Reference](docs/cli-reference.md)
- [Developer Notes](docs/developer-notes.md)

## What Is Implemented In This Repository

This repository contains the Rust workspace for Greentic Desktop. The current implementation includes:

- A runtime and CLI for local initialization, extension management, prompt planning, recording session lifecycle, runner discovery, replay, and MCP serving.
- Models for runner packages, portable desktop steps, replay validation, evidence, registry signing, security policy, deployment, rollout, and business flows.
- Built-in extension manifests for web, terminal, Windows UI Automation, Java accessibility, vision fallback, macOS accessibility, Linux X11, and Linux Wayland compatibility.
- Test coverage for the modeled workflows and local validation through CI.

Some feature areas are implemented as domain models and tests rather than a finished end-user desktop application. The docs describe the intended user workflow and call out the current command-line surface where it exists.

## Execution Truthfulness

Runner execution is visible when the selected adapter drives a visible desktop, browser, Java app, terminal, or remote viewport in the current user session. Web and terminal adapters can also run in controlled contexts that do not mirror every click in an unrelated user tab or shell. Native desktop recording and replay require the operating system permissions and session access listed in the adapter docs; if those checks fail, Greentic Desktop should block with a concrete reason instead of claiming success.

Release validation must fail if product runtime paths fabricate outputs such as `sample-output`, use fixed company names, or pass without declared inputs, secrets, output extractors, and evidence.

## Local Validation

For contributors, the main validation command is:

```bash
bash ci/local_check.sh
```

It runs formatting, linting, tests, builds, documentation generation, package checks, and publish dry-runs for publishable crates.

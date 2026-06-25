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

1. **Record the task once.** Start from a clear, repeatable task such as "create a customer in the CRM". Greentic Desktop observes the app, the fields you use, the buttons you press, and the checks that prove the task finished.
2. **Turn the recording into a runner.** The recording is normalized into a runner package. The runner keeps stable targets, inputs such as `company_name` or `email`, required secrets such as a login, expected outputs such as `customer_id`, and evidence rules.
3. **Review and approve the runner.** A person checks the runner before it is used by others. Published runners are expected to be signed so production systems do not run unapproved desktop automation.
4. **Expose it as an MCP tool forwarder.** The approved runner is converted into a tool with a stable name, input schema, output schema, permissions, approval policy, and evidence settings. A local or forwarded MCP client can then call that tool instead of driving the desktop directly.
5. **Use it from an assistant or workflow.** The caller supplies the required inputs, Greentic Desktop replays the desktop task, and the response includes structured outputs plus an evidence reference.

The current CLI can initialize the runtime, install adapter manifests, discover local `.gtpack` runner packages, and serve the MCP endpoint. The recording, runner conversion, approval, and forwarded-tool builder flows are implemented in this repository as product models and tests; user-facing commands for those authoring steps are still being added. See [Getting Started](docs/getting-started.md), [Recording and Refinement](docs/recording-and-refinement.md), [Runners](docs/runners.md), and [MCP Tools](docs/mcp-tools.md) for the detailed workflow.

## Current Command-Line Entry Points

For published releases, install the CLI with cargo-binstall:

```bash
cargo binstall greentic-desktop
```

From this repository, the current CLI can show runtime information, initialize local storage, manage built-in extensions, list local runner packages, and serve a small MCP endpoint.

```bash
greentic-desktop info
greentic-desktop init
greentic-desktop extension install greentic.desktop.playwright
greentic-desktop extension list
greentic-desktop runner list
greentic-desktop mcp serve
```

The same commands are also available through the `gtc desktop` form:

```bash
gtc desktop info
```

## Detailed Documentation

- [Getting Started](docs/getting-started.md)
- [Runners](docs/runners.md)
- [Adapters and Supported Desktops](docs/adapters.md)
- [Recording and Refinement](docs/recording-and-refinement.md)
- [MCP Tools](docs/mcp-tools.md)
- [Security, Approvals, and Secrets](docs/security.md)
- [Evidence and Audit Trail](docs/evidence.md)
- [Business Workflows](docs/business-workflows.md)
- [Deployment and Rollout](docs/deployment-and-rollout.md)
- [AWS WorkSpaces MCP Forwarding](docs/aws-workspaces-mcp.md)
- [CLI Reference](docs/cli-reference.md)
- [Developer Notes](docs/developer-notes.md)

## What Is Implemented In This Repository

This repository contains the Rust workspace for Greentic Desktop. The current implementation includes:

- A runtime and CLI for local initialization, extension management, runner discovery, and MCP serving.
- Models for runner packages, portable desktop steps, replay validation, evidence, registry signing, security policy, deployment, rollout, and business flows.
- Built-in extension manifests for web, terminal, Windows UI Automation, Java accessibility, vision fallback, macOS accessibility, Linux X11, and Linux Wayland compatibility.
- Test coverage for the modeled workflows and local validation through CI.

Some feature areas are implemented as domain models and tests rather than a finished end-user desktop application. The docs describe the intended user workflow and call out the current command-line surface where it exists.

## Local Validation

For contributors, the main validation command is:

```bash
bash ci/local_check.sh
```

It runs formatting, linting, tests, builds, documentation generation, package checks, and publish dry-runs for publishable crates.

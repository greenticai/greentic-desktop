# Runners

A runner is a reusable description of a desktop task. It says what the task needs, what steps to perform, what inputs and secrets are required, what outputs are expected, and which desktop capabilities must be available.

## What A Runner Contains

A runner package includes:

- **ID and version**: the stable name of the task, such as `crm.create_customer`.
- **Mode**: whether it came from a prompt, a human demonstration, or both.
- **Inputs**: values supplied by the caller, such as `email` or `company_name`.
- **Secrets**: sensitive values resolved at run time, such as passwords or tokens.
- **Steps**: actions such as opening an app, clicking a button, typing text, reading a field, or asserting that something is visible.
- **Assertions**: checks that confirm the run reached the expected state.
- **Outputs**: values returned to the caller, such as a created customer ID.

## Portable Runners

Some desktop tasks need to work across more than one operating system. Portable runners can keep platform-specific launch details and locators for Windows, macOS, Linux X11, and Linux Wayland.

For example, the same logical step can open the CRM app through:

- a Windows executable path,
- a macOS bundle ID,
- a Linux desktop file,
- and different element locators for each platform.

At replay time, Greentic Desktop selects the platform path that matches the current desktop.

## Runner Lifecycle

The intended lifecycle is:

1. Draft a runner from a prompt or demonstration.
2. Refine steps after user review.
3. Replay it against a compatible desktop.
4. Sign and publish it when approved.
5. Expose it as an MCP tool or use it inside a larger business workflow.
6. Keep evidence for each run.

## Signed Published Runners

Published runners are expected to be signed. By default, the runtime refuses unsigned published runner packages while still allowing unsigned drafts during local authoring.

This gives teams a way to experiment locally while keeping production automation controlled.

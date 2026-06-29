# Automate Hub GUI

`greentic-desktop` starts Automate Hub by default. It initializes the local runtime home, starts a loopback GUI server, opens the default browser, and serves the embedded frontend.

```bash
greentic-desktop
```

Use explicit GUI flags when testing:

```bash
greentic-desktop gui --no-open --bind 127.0.0.1:0
```

The startup URL includes a short-lived GUI token. The browser uses that token for mutating API calls. Do not share the URL outside the local machine.

## Home And Setup

The home page shows the runtime version, setup checklist, and recent activity. Use **Settings** to verify runtime folders, extension availability, desktop permissions, and MCP configuration.

## Create From Prompt

Open **Create**, choose the prompt workflow, describe the automation, then generate a draft. Prefer generic task language: open or attach to a target, provide typed inputs, issue a command such as save, observe/extract outputs, and assert success. A typical prompt is:

```text
Open a resource table, ask for resource_name, name, and email, append a row, save, and return saved_status.
```

The draft view shows typed inputs, required secrets, output extractors, required adapters, steps, policy warnings, test results, and the saved runner path. If the planner cannot infer a target app, output, or required secret, it should ask an open question before the runner is treated as ready.

## Record A Task

Open **Create**, choose recording, select a target such as Browser task or Desktop app task, and start recording. During review, mark inputs, outputs, secrets, assertions, and notes. Normalise the recording, test it with sample values, then finalise it into the runtime runner folder.

Settings includes an adapter readiness panel that reports installed adapters, sidecar/runtime readiness, executable capabilities, recordable targets, and the exact missing permission or dependency. For the target support matrix, see [Production Readiness Matrix](production-readiness.md).

Recording screens show whether a capture backend is active, blocked, paused, or stopped. For target-specific setup and troubleshooting, see [Recording Runbooks](recording-runbooks.md).

Recording only captures the target context owned or attached by the selected backend. Web recording captures the Greentic-owned browser context, not arbitrary existing tabs. Native desktop recording requires OS accessibility/event sources and screen permissions. If a backend is blocked, the UI should show the concrete missing permission or adapter source instead of a recording placeholder.

## Manage Runners

Open **My Runners** to see saved draft and packaged runners. Runner cards support run, edit, rename, delete, evidence links, and refinement for failed runners. Each runner is the MCP tool definition; there is no separate runner-vs-MCP lifecycle. Starting the managed MCP server exposes ready runners through MCP with the same input, secret, output, approval, and evidence checks used by the Run button.

## MCP Tools

Use **My Runners** to start, stop, or restart the managed MCP service. The MCP server should start automatically when configured. A runner cannot be "published as MCP" separately from the saved runner: Run and MCP calls use the same replay path and return the same output/evidence contract.

## Settings And Extensions

Open **Settings** to search recommended extensions, install or update adapters, enable or disable installed extensions, and test local LLM planning mode. See [Extension Store](extension-store.md) for source, trust, permission, and local store behavior.

## Troubleshooting

- If the browser does not open, run `greentic-desktop gui --no-open` and open the printed URL manually.
- If the GUI cannot bind, another process may be using the requested address. Use `--bind 127.0.0.1:0`.
- If MCP cannot start, check whether another process uses the configured MCP bind, usually `127.0.0.1:8799`.
- If a runner says an input, secret, adapter capability, or output extractor is missing, fix that declaration or setup item before retrying. A successful run should show returned outputs and an evidence reference.
- Runtime logs are written under the Greentic Desktop home folder, usually `~/.greentic/desktop/greentic-desktop.log`.
- Windows SmartScreen can warn for unsigned release binaries. See [Release And Installation](release.md).

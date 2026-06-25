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

Open **Create**, choose the prompt workflow, describe the automation, then generate a draft. The draft view shows inputs, outputs, required adapters, steps, policy warnings, test results, and the saved runner path.

## Record A Task

Open **Create**, choose recording, select a target such as Browser task or Desktop app task, and start recording. During review, mark inputs, outputs, secrets, assertions, and notes. Normalise the recording, test it, then finalise it into the runtime runner folder.

## Manage Runners

Open **My Runners** to see saved draft and packaged runners. Runner cards support run, test, publish as MCP, approval, evidence links, and refinement for failed runners. High-risk publish actions create an approval item before the runner becomes an MCP tool.

## MCP Tools

Open **MCP Tools** to start, stop, or restart the managed MCP service. Published runner tools appear with enable, disable, test, copy tool name, and client configuration controls.

## Settings And Extensions

Open **Settings** to search recommended extensions, install or update adapters, enable or disable installed extensions, and test local LLM planning mode. See [Extension Store](extension-store.md) for source, trust, permission, and local store behavior.

## Troubleshooting

- If the browser does not open, run `greentic-desktop gui --no-open` and open the printed URL manually.
- If the GUI cannot bind, another process may be using the requested address. Use `--bind 127.0.0.1:0`.
- If MCP cannot start, check whether another process uses the configured MCP bind, usually `127.0.0.1:8799`.
- Runtime logs are written under the Greentic Desktop home folder, usually `~/.greentic/desktop/greentic-desktop.log`.
- Windows SmartScreen can warn for unsigned release binaries. See [Release And Installation](release.md).

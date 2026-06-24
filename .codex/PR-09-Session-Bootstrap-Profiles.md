# PR-09 Session Bootstrap Profiles

## Goal

Define how a runner starts the target environment before replay.

Examples:

- Start local web server and open browser
- Open CRM desktop application
- Connect to TN3270 host
- Launch Excel workbook
- Attach to existing AWS WorkSpace session

## Session Profile Example: Local Web App

```yaml
id: local_web_app_test

bootstrap:
  - action: start_process
    command: npm
    args: ["run", "dev"]
    working_dir: "{{workspace_dir}}"
    output_ref: npm_dev_server

  - action: wait_for_http
    url: "http://localhost:5173"
    timeout_seconds: 60

  - action: open_browser
    browser: default
    url: "http://localhost:5173"

teardown:
  - action: stop_process
    ref: npm_dev_server
```

## Session Profile Example: Windows CRM

```yaml
id: windows_crm

bootstrap:
  - action: open_app
    path: "C:/Program Files/CRM/crm.exe"

  - action: wait_for_window
    title_contains: "CRM"
```

## Session Profile Example: Mainframe

```yaml
id: mainframe_customer_system

bootstrap:
  - action: terminal_connect
    protocol: tn3270
    host: "{{secrets.mainframe_host}}"
    port: 23
```

## Acceptance Criteria

- Runner can declare a session profile.
- Session profile can start and stop helper processes.
- Browser/web/local app profiles are supported.
- Terminal profiles are supported.
- Workspace attach mode is supported.

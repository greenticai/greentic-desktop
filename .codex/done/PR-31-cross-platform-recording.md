PR-31 — Cross-Platform Recording Format Upgrade

Goal: ensure recorded runners are portable across OSs where possible.

The existing roadmap already defines a portable runner package as a core concept. This PR should upgrade the recorded format so a runner can express:

platforms:
  supported:
    - windows
    - macos
    - linux-x11
  preferred_adapter:
    windows: greentic.desktop.windows.uia
    macos: greentic.desktop.macos.ax
    linux-x11: greentic.desktop.linux.x11

steps:
  - id: open_crm
    action: desktop.open_app
    app:
      windows:
        executable: "C:\\Program Files\\CRM\\crm.exe"
      macos:
        bundle_id: "com.vendor.crm"
      linux:
        desktop_file: "crm.desktop"
Acceptance criteria
A runner package can contain OS-specific locators.
A runner package can contain portable logical steps.
Replay chooses the correct platform adapter at runtime.
Unsupported OSs fail before execution.
Evidence shows which platform path was used.
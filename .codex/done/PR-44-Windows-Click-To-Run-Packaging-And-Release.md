# PR-44 - Windows Click-to-Run Packaging and Release

## Goal

Make the released Windows `.exe` support the intended click-to-run GUI experience while preserving CLI install and `cargo binstall greentic-desktop` behavior across Windows, macOS, and Linux.

## User Outcome

A Windows user can download `greentic-desktop.exe`, double-click it, and use the GUI. A command-line user can still install with `cargo binstall greentic-desktop` and run CLI commands.

## Current State

- Release workflow builds `greentic-desktop` and `gtc` binaries for Linux/macOS/Windows x64 and ARM.
- `cargo-binstall` metadata points to GitHub release archives.
- The repo is currently private, which blocks public unauthenticated binstall release asset downloads.
- No GUI-mode packaging behavior exists yet.

## Scope

1. Decide Windows executable subsystem strategy.
2. Ensure release archives include GUI assets if assets are not embedded.
3. Keep `cargo-binstall` metadata accurate.
4. Add a direct downloadable `.exe` or archive UX for Windows.
5. Add release smoke tests for GUI startup.
6. Document public/private repo implications for `cargo binstall`.

## Windows Behavior

Options:

### Option A - Single Console Binary

Keep one `greentic-desktop.exe` as a console binary.

Pros:

- simplest
- CLI output works naturally
- current packaging remains straightforward

Cons:

- double-click may show a console window

### Option B - Separate GUI Binary

Add a second binary, for example `greentic-desktop-gui.exe`, with Windows subsystem set to `windows`.

Pros:

- clean double-click experience

Cons:

- `cargo binstall greentic-desktop` needs clear binary selection
- release archives carry more binaries
- docs must explain `greentic-desktop` versus `gtc`

### Recommendation

Start with Option A unless user testing proves the console window is unacceptable. The critical product behavior is opening the default browser and working locally; subsystem polish can be a follow-up.

## Release Requirements

- Release archives must contain:
  - `greentic-desktop(.exe)`
  - `gtc(.exe)`
  - license/readme
  - any external GUI assets only if not embedded
- `package.metadata.binstall.bin-dir` must match archive layout.
- Archives must remain named:
  - `greentic-desktop-v<version>-<target>.tgz`
  - `greentic-desktop-v<version>-<target>.zip`
- `cargo binstall greentic-desktop` must download from a public GitHub release or another public asset host.

## GitHub/Crates.io Publication Notes

For public `cargo binstall`:

- crate metadata must be on crates.io
- all internal dependency crates must be published first
- GitHub release assets must be publicly accessible
- if the repo remains private, binstall cannot anonymously fetch assets from GitHub releases

If private distribution is required, document an authenticated alternative and do not market it as normal `cargo binstall`.

## CI Additions

Add release smoke checks:

- start `greentic-desktop gui --no-open --bind 127.0.0.1:0`
- verify `/` serves HTML
- verify `/api/v1/health`
- verify process shuts down cleanly

For Windows:

- run GUI startup smoke test on Windows x64
- optionally run ARM Windows startup if runner availability/cost is acceptable

## Acceptance Criteria

- Windows release artifact can be downloaded and launched.
- Double-click opens browser to Automate Hub.
- CLI commands still work from terminal.
- `cargo binstall greentic-desktop` installs the expected binary from public release assets.
- Release docs explain both direct download and binstall.

## Test Plan

- Manual Windows x64 double-click test.
- Manual Windows ARM test where hardware/runner is available.
- `cargo binstall --no-confirm greentic-desktop` against a published release.
- Run GUI smoke checks on each release target.

## Risks

- Public binstall is incompatible with private-only GitHub release assets.
- Windows SmartScreen may warn on unsigned binaries. Code signing is outside this PR unless certificates are available.


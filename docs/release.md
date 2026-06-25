# Release And Installation

Greentic Desktop release archives are named:

- `greentic-desktop-v<version>-<target>.tgz` for Linux and macOS
- `greentic-desktop-v<version>-<target>.zip` for Windows

Each archive contains:

- `greentic-desktop` or `greentic-desktop.exe`
- `gtc` or `gtc.exe`
- `README.md`
- `LICENSE`

The Automate Hub frontend is embedded into the Rust binary during release builds, so no external GUI asset folder is required in the archive.

## Windows Click-To-Run

Windows releases use the single console binary strategy. A user can download the Windows zip, extract it, and double-click `greentic-desktop.exe`. The executable starts the local Automate Hub GUI, opens the default browser, and keeps serving from a loopback address until the process exits.

The same `greentic-desktop.exe` also works from PowerShell or Command Prompt:

```powershell
.\greentic-desktop.exe gui --no-open --bind 127.0.0.1:0
.\greentic-desktop.exe info
.\gtc.exe desktop info
```

Windows may show SmartScreen warnings for unsigned binaries. Code signing is separate from the archive layout and is not assumed by the release workflow.

## cargo-binstall

`cargo binstall greentic-desktop` uses the package metadata in `crates/greentic-desktop-cli/Cargo.toml` and expects the GitHub release archives above.

Public unauthenticated binstall requires:

- the `greentic-desktop` crate metadata to be available on crates.io,
- all internal dependency crates to be published first,
- GitHub release assets to be publicly accessible,
- archive layout to keep binaries under `greentic-desktop-v<version>-<target>/`.

If the repository or release assets remain private, normal unauthenticated `cargo binstall greentic-desktop` cannot fetch them. Private distribution should use an authenticated download path or a private package registry and should not be described as standard public binstall.

## Release Smoke Checks

The publish workflow starts the target `greentic-desktop` binary with:

```bash
greentic-desktop gui --no-open --bind 127.0.0.1:0
```

It verifies that `/` serves Automate Hub HTML and `/api/v1/health` returns `status: ok`, then shuts the process down before packaging.

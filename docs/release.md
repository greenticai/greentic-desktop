# Release And Installation

Greentic Desktop release archives are named:

- `greentic-desktop-v<version>-<target>.tgz` for Linux and macOS
- `greentic-desktop-v<version>-<target>.zip` for Windows

The release also publishes:

- `checksums.txt`

`checksums.txt` contains SHA-256 checksums for every release archive. The one-line installers verify the downloaded archive when this file is present.

Each archive contains:

- `greentic-desktop` or `greentic-desktop.exe`
- `gtc` or `gtc.exe`
- `README.md`
- `LICENSE`

The Automate Hub frontend is embedded into the Rust binary during release builds, so no external GUI asset folder is required in the archive.

## One-Line Installers

Public releases can be installed without Rust or cargo-binstall:

```bash
curl -fsSL https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.sh | sh
```

```powershell
irm https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.ps1 | iex
```

The installers use the GitHub Releases API to discover the latest release by default. They also support exact tags:

```bash
GREENTIC_DESKTOP_VERSION=v0.1.9 curl -fsSL https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.sh | sh
```

```powershell
$env:GREENTIC_DESKTOP_VERSION="v0.1.9"; irm https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.ps1 | iex
```

Supported targets:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`
- `aarch64-pc-windows-msvc`

See [One-Line Install](install-one-line.md) for install locations, environment variables, and uninstall instructions.

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

Before cutting a release, run:

```bash
bash ci/no_mock_production_check.sh
bash ci/local_check.sh
```

Confirm the support matrix in [Production Readiness Matrix](production-readiness.md) still matches `/api/v1/adapters/health`.

The publish workflow starts the target `greentic-desktop` binary with:

```bash
greentic-desktop gui --no-open --bind 127.0.0.1:0
```

It verifies that `/` serves Automate Hub HTML and `/api/v1/health` returns `status: ok`, then shuts the process down before packaging.

Release validation must also keep the archive names above stable because `install.sh`, `install.ps1`, and `cargo binstall` all depend on that layout.

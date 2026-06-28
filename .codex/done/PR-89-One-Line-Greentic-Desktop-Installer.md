# PR 89: One-Line Greentic Desktop Installer

## Summary

Add a zero-Rust, zero-cargo-binstall installation path for Greentic Desktop from public GitHub release archives.

Target UX:

```bash
curl -fsSL https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.sh | sh
```

```powershell
irm https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.ps1 | iex
```

After install, users can run:

```bash
greentic-desktop
gtc desktop info
```

## Current Repo Corrections

- The release workflow is `.github/workflows/publish.yml`, not `release.yml`.
- The workflow already builds these archive names:
  - `greentic-desktop-v<version>-x86_64-unknown-linux-gnu.tgz`
  - `greentic-desktop-v<version>-aarch64-unknown-linux-gnu.tgz`
  - `greentic-desktop-v<version>-x86_64-apple-darwin.tgz`
  - `greentic-desktop-v<version>-aarch64-apple-darwin.tgz`
  - `greentic-desktop-v<version>-x86_64-pc-windows-msvc.zip`
  - `greentic-desktop-v<version>-aarch64-pc-windows-msvc.zip`
- The workflow does not currently publish `checksums.txt`; add that by collecting release archive artifacts and uploading one aggregate checksum file.
- Automate Hub is embedded in the release binary, so installers only need the archive binaries.

## Scope

Implement:

- `install.sh`
- `install.ps1`
- `docs/install-one-line.md`

Update:

- `README.md`
- `docs/release.md`
- `.github/workflows/publish.yml`
- `.github/workflows/ci.yml`
- `ci/local_check.sh`

## Installer Requirements

Both installers must:

- detect OS/architecture and map to the release target triple,
- use latest GitHub release by default,
- support exact release tag through `GREENTIC_DESKTOP_VERSION`,
- support `GREENTIC_DESKTOP_REPO`,
- support custom install/bin dirs,
- download the matching archive from GitHub Releases,
- verify `checksums.txt` when present,
- install `greentic-desktop` and `gtc` into a user-local directory,
- run first-time init unless `GREENTIC_DESKTOP_NO_INIT=1`,
- fail clearly for unsupported OS/architecture,
- avoid sudo/admin requirements.

## Platform Defaults

Linux/macOS:

- install dir: `~/.greentic/desktop/bin`
- bin dir: `~/.local/bin`
- symlink both binaries into the bin dir
- print PATH instructions if bin dir is not on `PATH`

Windows:

- install dir: `%LOCALAPPDATA%\Greentic\Desktop\bin`
- add install dir to User PATH if missing
- update current process PATH

## Tests

Add syntax checks:

- `sh -n install.sh`
- PowerShell parser check when `pwsh` is available

These should run locally through `ci/local_check.sh` and in CI without live GitHub API calls.

## Acceptance Criteria

- One-line install docs are visible from README.
- macOS/Linux installer supports latest, exact version, custom install dirs, checksum verification, and no-init mode.
- Windows installer supports latest, exact version, custom install dir, checksum verification, User PATH update, and no-init mode.
- Release workflow publishes `checksums.txt`.
- CI validates installer syntax.
- Existing cargo-binstall and manual archive installation docs remain available.

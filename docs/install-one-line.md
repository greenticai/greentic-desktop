# One-Line Install

Greentic Desktop can be installed from public GitHub release archives without Rust, Cargo, cargo-binstall, Homebrew, Chocolatey, or administrator rights.

The installers download the matching release archive for your platform, verify `checksums.txt` when the release provides it, install the `greentic-desktop` and `gtc` binaries into a user-local directory, and run first-time initialization.

## macOS and Linux

Install the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.sh | sh
```

Install a specific release tag:

```bash
GREENTIC_DESKTOP_VERSION=v0.1.6 curl -fsSL https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.sh | sh
```

Skip first-time initialization:

```bash
GREENTIC_DESKTOP_NO_INIT=1 curl -fsSL https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.sh | sh
```

Default locations:

```text
Install dir: ~/.greentic/desktop/bin
Bin dir:     ~/.local/bin
```

Installed files:

```text
~/.greentic/desktop/bin/greentic-desktop
~/.greentic/desktop/bin/gtc
~/.local/bin/greentic-desktop -> ~/.greentic/desktop/bin/greentic-desktop
~/.local/bin/gtc -> ~/.greentic/desktop/bin/gtc
```

If `~/.local/bin` is not on `PATH`, add this to your shell profile:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

## Windows PowerShell

Install the latest release:

```powershell
irm https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.ps1 | iex
```

Install a specific release tag:

```powershell
$env:GREENTIC_DESKTOP_VERSION="v0.1.6"; irm https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.ps1 | iex
```

Skip first-time initialization:

```powershell
$env:GREENTIC_DESKTOP_NO_INIT="1"; irm https://raw.githubusercontent.com/greenticai/greentic-desktop/main/install.ps1 | iex
```

Default location:

```text
%LOCALAPPDATA%\Greentic\Desktop\bin
```

Installed files:

```text
%LOCALAPPDATA%\Greentic\Desktop\bin\greentic-desktop.exe
%LOCALAPPDATA%\Greentic\Desktop\bin\gtc.exe
```

The installer adds the install directory to the User PATH. Already-open terminals may need to be restarted.

## Configuration

Both installers support:

```text
GREENTIC_DESKTOP_REPO
GREENTIC_DESKTOP_VERSION
GREENTIC_DESKTOP_INSTALL_DIR
GREENTIC_DESKTOP_NO_INIT
```

`install.sh` also supports:

```text
GREENTIC_DESKTOP_BIN_DIR
```

Defaults:

```text
GREENTIC_DESKTOP_REPO=greenticai/greentic-desktop
GREENTIC_DESKTOP_VERSION=latest
```

## Supported Release Targets

The installers map the current platform to these release assets:

```text
x86_64-unknown-linux-gnu
aarch64-unknown-linux-gnu
x86_64-apple-darwin
aarch64-apple-darwin
x86_64-pc-windows-msvc
aarch64-pc-windows-msvc
```

Linux and macOS use `.tgz` archives. Windows uses `.zip` archives.

## Verify Installation

```bash
greentic-desktop info
gtc desktop info
```

Start Automate Hub:

```bash
greentic-desktop
```

## Uninstall

macOS/Linux:

```bash
rm -f ~/.local/bin/greentic-desktop ~/.local/bin/gtc
rm -rf ~/.greentic/desktop/bin
```

Optional full data removal:

```bash
rm -rf ~/.greentic/desktop
```

Windows:

```powershell
Remove-Item "$env:LOCALAPPDATA\Greentic\Desktop\bin" -Recurse -Force
```

Remove the install directory from the User PATH:

```powershell
$installDir = "$env:LOCALAPPDATA\Greentic\Desktop\bin"
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$newPath = (($userPath -split ";") | Where-Object { $_ -and $_ -ne $installDir }) -join ";"
[Environment]::SetEnvironmentVariable("Path", $newPath, "User")
```

Optional full data removal:

```powershell
Remove-Item "$env:LOCALAPPDATA\Greentic\Desktop" -Recurse -Force
```

## Troubleshooting

- `greentic-desktop: command not found`: make sure the bin directory is on `PATH`, then restart your terminal.
- Unsupported OS/architecture: download a matching archive manually from the GitHub release or build from source.
- Checksum failure: remove the partially downloaded file and retry. Do not run a binary whose checksum failed.
- Private or enterprise releases: set `GREENTIC_DESKTOP_REPO` only for public repositories. Authenticated enterprise distribution is a separate installation path.

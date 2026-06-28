$ErrorActionPreference = "Stop"

$Repo = if ($env:GREENTIC_DESKTOP_REPO) { $env:GREENTIC_DESKTOP_REPO } else { "greenticai/greentic-desktop" }
$Version = if ($env:GREENTIC_DESKTOP_VERSION) { $env:GREENTIC_DESKTOP_VERSION } else { "latest" }

if (-not $env:LOCALAPPDATA -and -not $env:GREENTIC_DESKTOP_INSTALL_DIR) {
    throw "LOCALAPPDATA is not set. Set GREENTIC_DESKTOP_INSTALL_DIR to choose an install directory."
}

$InstallDir = if ($env:GREENTIC_DESKTOP_INSTALL_DIR) {
    $env:GREENTIC_DESKTOP_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "Greentic\Desktop\bin"
}

$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
switch ($Arch) {
    "X64" { $Target = "x86_64-pc-windows-msvc" }
    "Arm64" { $Target = "aarch64-pc-windows-msvc" }
    default { throw "Unsupported Windows architecture: $Arch" }
}

$ApiBase = "https://api.github.com/repos/$Repo/releases"
if ($Version -eq "latest") {
    $ReleaseUrl = "$ApiBase/latest"
} else {
    $ReleaseUrl = "$ApiBase/tags/$Version"
}

$Temp = Join-Path ([System.IO.Path]::GetTempPath()) ("greentic-desktop-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $Temp | Out-Null

try {
    Write-Host "Resolving Greentic Desktop release for $Target..."
    $Release = Invoke-RestMethod -Uri $ReleaseUrl -Headers @{ "User-Agent" = "greentic-desktop-installer" }
    $Asset = $Release.assets | Where-Object {
        $_.name -like "greentic-desktop-v*-$Target.zip"
    } | Select-Object -First 1
    if (-not $Asset) {
        throw "No Greentic Desktop release asset found for target $Target in $Repo."
    }

    $Archive = Join-Path $Temp $Asset.name
    Write-Host "Downloading $($Asset.name)..."
    Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $Archive -Headers @{ "User-Agent" = "greentic-desktop-installer" }

    $ChecksumAsset = $Release.assets | Where-Object { $_.name -eq "checksums.txt" } | Select-Object -First 1
    if ($ChecksumAsset) {
        $Checksums = Join-Path $Temp "checksums.txt"
        Invoke-WebRequest -Uri $ChecksumAsset.browser_download_url -OutFile $Checksums -Headers @{ "User-Agent" = "greentic-desktop-installer" }
        $Line = Get-Content $Checksums | Where-Object { $_ -match ("\s\*?" + [regex]::Escape($Asset.name) + "$") } | Select-Object -First 1
        if ($Line) {
            $Expected = ($Line -split "\s+")[0].ToLowerInvariant()
            $Actual = (Get-FileHash -Algorithm SHA256 -Path $Archive).Hash.ToLowerInvariant()
            if ($Expected -ne $Actual) {
                throw "Checksum verification failed for $($Asset.name)."
            }
        } else {
            Write-Warning "checksums.txt did not contain $($Asset.name); skipping checksum verification."
        }
    } else {
        Write-Warning "checksums.txt was not found on the release; continuing without checksum verification."
    }

    $ExtractDir = Join-Path $Temp "extract"
    New-Item -ItemType Directory -Force -Path $ExtractDir | Out-Null
    Expand-Archive -Path $Archive -DestinationPath $ExtractDir -Force

    $DesktopExe = Get-ChildItem -Path $ExtractDir -Recurse -File -Filter "greentic-desktop.exe" | Select-Object -First 1
    $GtcExe = Get-ChildItem -Path $ExtractDir -Recurse -File -Filter "gtc.exe" | Select-Object -First 1
    if (-not $DesktopExe -or -not $GtcExe) {
        throw "Release archive did not contain greentic-desktop.exe and gtc.exe."
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item $DesktopExe.FullName (Join-Path $InstallDir "greentic-desktop.exe") -Force
    Copy-Item $GtcExe.FullName (Join-Path $InstallDir "gtc.exe") -Force

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $PathEntries = @()
    if ($UserPath) {
        $PathEntries = $UserPath -split ";"
    }
    if (-not ($PathEntries | Where-Object { $_ -eq $InstallDir })) {
        $NewUserPath = if ($UserPath) { "$UserPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("Path", $NewUserPath, "User")
    }
    if (-not (($env:Path -split ";") | Where-Object { $_ -eq $InstallDir })) {
        $env:Path = "$env:Path;$InstallDir"
    }

    if ($env:GREENTIC_DESKTOP_NO_INIT -ne "1") {
        & (Join-Path $InstallDir "greentic-desktop.exe") init
    }

    Write-Host ""
    Write-Host "Greentic Desktop installed successfully."
    Write-Host "Installed binaries: $InstallDir"
    Write-Host "User PATH was updated. Already-open terminals may need to be restarted."
    Write-Host ""
    Write-Host "Try:"
    Write-Host "  greentic-desktop"
    Write-Host "  gtc desktop info"
} finally {
    Remove-Item $Temp -Recurse -Force -ErrorAction SilentlyContinue
}

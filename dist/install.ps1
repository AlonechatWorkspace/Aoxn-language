<#
.SYNOPSIS
  Aoxn one-click installer (Windows x86_64).

.DESCRIPTION
  Downloads the prebuilt Aoxn toolchain archive for Windows, unpacks it into a
  self-contained directory, adds it to the user PATH, makes sure a C toolchain
  (clang) is available for the backend, and finishes with `aoxn doctor`.

  Run it with one of:
    irm https://raw.githubusercontent.com/AlonechatWorkspace/Aoxn-language/main/dist/install.ps1 | iex
    powershell -ExecutionPolicy Bypass -File install.ps1
    powershell -File install.ps1 -Archive .\aoxn-v0.30.0-windows-x86_64.zip

.PARAMETER Version
  Install a specific version (default: the latest GitHub release).

.PARAMETER Prefix
  Install root (default: $env:LOCALAPPDATA\aoxn, or $env:AOXN_HOME when set).

.PARAMETER Archive
  Install from a local .zip or an already-unpacked directory (offline / testing).

.PARAMETER NoClang
  Do not try to install the C toolchain.

.PARAMETER NoPath
  Do not modify the user PATH.

.PARAMETER Force
  Overwrite an existing install without asking.

.PARAMETER Uninstall
  Remove the install root and the PATH entry.
#>
[CmdletBinding()]
param(
    [string] $Version = "",
    [string] $Prefix = "",
    [string] $Archive = "",
    [switch] $NoClang,
    [switch] $NoPath,
    [switch] $Force,
    [switch] $Uninstall
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = "AlonechatWorkspace/Aoxn-language"

function Say  { param($m) Write-Host "==> $m" -ForegroundColor Cyan }
function Warn { param($m) Write-Host "warning: $m" -ForegroundColor Yellow }
function Die  { param($m) Write-Host "error: $m" -ForegroundColor Red; exit 1 }

if ($Uninstall) {
    $root = if ($Prefix) { $Prefix } elseif ($env:AOXN_HOME) { $env:AOXN_HOME } else { Join-Path $env:LOCALAPPDATA "aoxn" }
    Say "removing $root"
    if (Test-Path $root) { Remove-Item -Recurse -Force $root }
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -and $userPath.Contains("$root\bin")) {
        $new = ($userPath -split ";" | Where-Object { $_ -and $_.TrimEnd("\") -ne "$root\bin" }) -join ";"
        [Environment]::SetEnvironmentVariable("Path", $new, "User")
        Say "removed $root\bin from the user PATH"
    }
    Say "done"
    exit 0
}

$arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "x86" }
if ($arch -ne "x86_64") { Die "only 64-bit Windows is supported (found $arch)" }

if (-not $Prefix) {
    $Prefix = if ($env:AOXN_HOME) { $env:AOXN_HOME } else { Join-Path $env:LOCALAPPDATA "aoxn" }
}
$Prefix = $Prefix.TrimEnd("\")
$AoxnExe = Join-Path $Prefix "bin\aoxn.exe"

# ---- obtain the archive ------------------------------------------------------
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("aoxn-install-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp -Force | Out-Null
try {
    if ($Archive) {
        if (-not (Test-Path $Archive)) { Die "archive not found: $Archive" }
        Say "installing from local archive $Archive"
        $src = (Resolve-Path $Archive).Path
    } else {
        if (-not $Version) {
            Say "looking up the latest Aoxn release"
            try {
                $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "aoxn-installer" }
                $Version = ($rel.tag_name -replace "^v", "")
            } catch {
                Die "could not reach GitHub ($($_.Exception.Message)); pass -Version <v> or -Archive <path>"
            }
        }
        $asset = "aoxn-v$Version-windows-x86_64.zip"
        $url = "https://github.com/$Repo/releases/download/v$Version/$asset"
        Say "downloading $asset"
        $src = Join-Path $tmp $asset
        try {
            Invoke-WebRequest -Uri $url -OutFile $src -UseBasicParsing
        } catch {
            Die "download failed: $url ($($_.Exception.Message))"
        }
    }

    # ---- unpack --------------------------------------------------------------
    $stage = Join-Path $tmp "stage"
    New-Item -ItemType Directory -Path $stage -Force | Out-Null
    Say "unpacking"
    if ((Get-Item $src).PSIsContainer) {
        Copy-Item -Recurse -Force (Join-Path $src "*") $stage
    } else {
        Expand-Archive -Path $src -DestinationPath $stage -Force
    }
    if (-not (Test-Path (Join-Path $stage "bin\aoxn.exe"))) {
        Die "archive does not contain bin\aoxn.exe"
    }

    Say "installing to $Prefix"
    New-Item -ItemType Directory -Path (Join-Path $Prefix "bin") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $Prefix "lib") -Force | Out-Null
    if (Test-Path (Join-Path $Prefix "lib\stdlib")) { Remove-Item -Recurse -Force (Join-Path $Prefix "lib\stdlib") }
    Copy-Item -Recurse -Force (Join-Path $stage "*") $Prefix
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

# ---- PATH --------------------------------------------------------------------
if (-not $NoPath) {
    $binDir = Join-Path $Prefix "bin"
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $userPath) { $userPath = "" }
    $parts = @($userPath -split ";" | Where-Object { $_ })
    if (-not ($parts | Where-Object { $_.TrimEnd("\") -ieq $binDir })) {
        $new = (@($parts) + $binDir) -join ";"
        # NOT setx: it truncates at 1024 characters and silently corrupts PATH
        [Environment]::SetEnvironmentVariable("Path", $new, "User")
        Say "added $binDir to the user PATH (new terminals only)"
    }
}

# ---- C toolchain -------------------------------------------------------------
function Find-Clang {
    $cmd = Get-Command clang.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($p in @("C:\Program Files\LLVM\bin\clang.exe", "$Prefix\toolchain\bin\clang.exe")) {
        if (Test-Path $p) { return $p }
    }
    return $null
}

if (Find-Clang) {
    Say "clang found: $(Find-Clang)"
} elseif ($NoClang) {
    Warn "no clang found and -NoClang was given"
    Warn "the C backend needs one: winget install LLVM.LLVM  (plus MSVC Build Tools for linking)"
} else {
    Say "no clang found - installing LLVM (this backend compiles the generated C)"
    $winget = Get-Command winget -ErrorAction SilentlyContinue
    if ($winget) {
        try {
            & winget install --id LLVM.LLVM --exact --accept-source-agreements --accept-package-agreements --silent | Out-Null
            Say "winget installed LLVM"
        } catch {
            Warn "winget could not install LLVM: $($_.Exception.Message)"
        }
    } else {
        Warn "winget not available; install LLVM manually: https://releases.llvm.org/ (and the MSVC Build Tools)"
    }
    $machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
    if ($machinePath -and (Test-Path "C:\Program Files\LLVM\bin\clang.exe")) {
        $env:Path = "C:\Program Files\LLVM\bin;" + $env:Path
    }
}

# ---- verify ------------------------------------------------------------------
Say "running 'aoxn doctor'"
& $AoxnExe doctor
$doctorExit = $LASTEXITCODE

Write-Host ""
Say "Aoxn installed: $AoxnExe"
& $AoxnExe version
Write-Host ""
Say "try it:"
Write-Host "    & `"$AoxnExe`" run `"$Prefix\examples\hello.ax`""
Write-Host "    (or just 'aoxn run ...' in a NEW terminal)"
exit $doctorExit
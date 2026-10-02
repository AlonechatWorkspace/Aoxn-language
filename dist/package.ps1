<#
.SYNOPSIS
  Build the single-file Aoxn installer for Windows (Aoxn-<version>-Setup.exe).

.DESCRIPTION
  Produces ONE self-contained executable that installs the whole toolchain:
  the compiler, the standard library and the examples. It is the
  `aoxn-setup` stub (src/setup/main.rs) with a stored archive appended after
  its PE image; the user downloads that one file, double-clicks it, and ends
  up with `aoxn` on PATH.

  Run from the repository root:

      powershell -ExecutionPolicy Bypass -File dist\package.ps1
      powershell -ExecutionPolicy Bypass -File dist\package.ps1 -Output C:\Temp
#>
[CmdletBinding()]
param(
    [string] $Output = "dist\release",
    [switch] $SkipBuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Push-Location $Root
try {
    $Version = (Select-String -Path "Cargo.toml" -Pattern '^version = "(.+)"$').Matches[0].Groups[1].Value
    Write-Host "==> Aoxn v$Version"

    if (-not $SkipBuild) {
        Write-Host "==> building (release)"
        cargo build --release --locked --bin aoxn --bin aoxn-setup
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }

    $Stub = "target\release\aoxn-setup.exe"
    $Compiler = "target\release\aoxn.exe"
    foreach ($f in @($Stub, $Compiler)) {
        if (-not (Test-Path $f)) { throw "missing $f (run without -SkipBuild)" }
    }

    # ---- collect the payload --------------------------------------------------
    # Same layout the compiler discovers at runtime (src/paths.rs):
    #   bin/aoxn.exe   lib/stdlib/*.ax   examples/*.ax   docs/install.md   LICENSE  README.md
    $Files = @(
        @{ Path = $Compiler;          Name = "bin/aoxn.exe" },
        @{ Path = "CHANGELOG.md";     Name = "CHANGELOG.md" },
        @{ Path = "LICENSE";          Name = "LICENSE" },
        @{ Path = "README.md";        Name = "README.md" }
    )
    Get-ChildItem "stdlib\*.ax"    | ForEach-Object { $Files += @{ Path = $_.FullName; Name = "lib/stdlib/$($_.Name)" } }
    Get-ChildItem "examples\*.ax"  | ForEach-Object { $Files += @{ Path = $_.FullName; Name = "examples/$($_.Name)" } }
    if (Test-Path "docs\install.md") { $Files += @{ Path = "docs\install.md"; Name = "docs/install.md" } }

    Write-Host "==> building the payload ($($Files.Count) files, uncompressed)"
    $Buffer = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter($Buffer)
    $bw.Write([UInt32]$Files.Count)
    foreach ($f in $Files) {
        if (-not (Test-Path $f.Path)) { throw "payload file missing: $($f.Path)" }
        $bytes = [IO.File]::ReadAllBytes($f.Path)
        $name = [Text.Encoding]::UTF8.GetBytes(($f.Name -replace '\\', '/'))
        $bw.Write([UInt16]$name.Length)
        $bw.Write($name)
        $bw.Write([UInt64]$bytes.Length)
        $bw.Write([Byte](1 -and ($f.Name -like "bin/*")))   # executable flag
        $bw.Write($bytes)
    }
    $bw.Flush()
    $payload = $Buffer.ToArray()

    # ---- append it to the stub ------------------------------------------------
    Write-Host "==> assembling the installer"
    New-Item -ItemType Directory -Force -Path $Output | Out-Null
    $OutFile = Join-Path $Output "Aoxn-$Version-Setup.exe"
    Copy-Item $Stub $OutFile -Force
    $fs = [IO.File]::Open($OutFile, [IO.FileMode]::Append)
    try {
        $fs.Write($payload, 0, $payload.Length)
        $tail = New-Object System.IO.BinaryWriter($fs)
        $tail.Write([UInt64]$payload.Length)
        $tail.Write([byte[]](0x41,0x4F,0x58,0x4E,0x53,0x46,0x58,0x00))  # "AOXNSFX" + NUL
        $tail.Flush()
    } finally {
        $fs.Close()
    }

    $hash = (Get-FileHash -Algorithm SHA256 $OutFile).Hash.ToLower()
    "$hash  $(Split-Path -Leaf $OutFile)" | Out-File -Encoding ascii (Join-Path $Output "Aoxn-$Version-Setup.exe.sha256")

    $sizeMB = [Math]::Round((Get-Item $OutFile).Length / 1MB, 2)
    Write-Host ""
    Write-Host "==> $OutFile ($sizeMB MB)" -ForegroundColor Green
    Write-Host "    sha256 $hash"
    Write-Host "    double-click it to install; it adds aoxn to PATH and runs 'aoxn doctor'"
} finally {
    Pop-Location
}
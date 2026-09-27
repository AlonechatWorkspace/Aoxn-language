$ErrorActionPreference = "Stop"
$wiki = Join-Path (Get-Location) "wiki"
$files = Get-ChildItem $wiki -Filter *.md | Sort-Object Name
$problems = @()

# \u4e2d\u6587 = the two CJK chars for "Chinese"; kept as escapes so this
# script stays pure ASCII (PowerShell 5.1 reads .ps1 without BOM as ANSI).
$zhHeading = '(?m)^## \u4e2d\u6587\s*$'

foreach ($f in $files) {
  $raw = [System.IO.File]::ReadAllBytes($f.FullName)
  if ($raw.Length -ge 3 -and $raw[0] -eq 0xEF -and $raw[1] -eq 0xBB -and $raw[2] -eq 0xBF) {
    $problems += "BOM: $($f.Name)"
  }
  $text = [System.IO.File]::ReadAllText($f.FullName, [System.Text.Encoding]::UTF8)
  $lines = $text -split "`r?`n"

  if ($f.Name -notlike "_*") {
    if ($lines[0] -notmatch '^# .+ \u00b7 .+$') { $problems += "TITLE: $($f.Name) -> '$($lines[0])'" }
    if ($text -notmatch $zhHeading) { $problems += "NO-ZH: $($f.Name)" }
    if ($text -notmatch '(?m)^## English\s*$') { $problems += "NO-EN: $($f.Name)" }
    if ($text -notmatch 'Source files') { $problems += "NO-REFS: $($f.Name)" }
  }

  foreach ($m in [regex]::Matches($text, '\]\(([^)]+)\)')) {
    $t = $m.Groups[1].Value
    if ($t -match '^https?://') { continue }
    if ($t -match '^#') { continue }
    if ($t -notmatch '\.md$') { continue }
    $resolved = Join-Path $wiki $t
    if (-not (Test-Path -LiteralPath $resolved)) {
      $problems += "DEAD-LINK: $($f.Name) -> $t"
    }
  }
}

Write-Output "files: $($files.Count)"
if ($problems.Count -eq 0) { Write-Output "OK: no problems" } else { $problems | ForEach-Object { Write-Output "PROBLEM $_" } }

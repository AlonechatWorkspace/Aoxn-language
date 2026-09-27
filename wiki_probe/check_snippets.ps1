$ErrorActionPreference = "SilentlyContinue"
$root = Get-Location
$outDir = Join-Path $root "wiki_probe\snips"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$exe = Join-Path $root "target\debug\aoxn.exe"

$snips = @()
foreach ($f in (Get-ChildItem (Join-Path $root "wiki") -Filter *.md | Sort-Object Name)) {
  $text = [System.IO.File]::ReadAllText($f.FullName, [System.Text.Encoding]::UTF8)
  $blocks = [regex]::Matches($text, '(?s)```aoxn\r?\n(.*?)```')
  $i = 0
  foreach ($b in $blocks) {
    $i++
    $code = $b.Groups[1].Value
    $code = $code -replace '"\.\./stdlib/', '"../../stdlib/'
    $name = "$($f.BaseName)_$i"
    $path = Join-Path $outDir "$name.ax"
    [System.IO.File]::WriteAllText($path, $code, (New-Object System.Text.UTF8Encoding($false)))
    $snips += [pscustomobject]@{ Page = $f.BaseName; N = $i; Path = $path; Code = $code }
  }
}

Write-Output ("aoxn blocks found: " + $snips.Count)
Write-Output ("blocks containing 'def main(' : " + (($snips | Where-Object { $_.Code -match 'def main\(' }).Count))

function Try-Run($code, $path) {
  [System.IO.File]::WriteAllText($path, $code, (New-Object System.Text.UTF8Encoding($false)))
  $out = & $exe run $path 2>&1 | ForEach-Object { "$_" }
  return @{ Ok = ($LASTEXITCODE -eq 0); Err = (($out | Where-Object { $_ -match '^\[' } | Select-Object -First 1)) }
}

$fails = @()
$modes = @{}
foreach ($s in $snips) {
  $code = $s.Code
  $candidates = @()
  if ($code -match 'def main\(') {
    $candidates += ,@("asis", $code)
  } else {
    $candidates += ,@("decl", ($code + "`ndef main() -> int:`n    return 0`n"))
    $lines = $code -split "`n"
    $imports = ($lines | Where-Object { $_ -match '^\s*import\s' }) -join "`n"
    $rest = ($lines | Where-Object { $_ -notmatch '^\s*import\s' })
    $indented = ($rest | ForEach-Object { if ($_ -match '^\s*$') { "" } else { "    " + $_ } }) -join "`n"
    $wrapped = ""
    if ($imports -ne "") { $wrapped += $imports + "`n" }
    $wrapped += "def main() -> int:`n" + $indented + "`n    return 0`n"
    $candidates += ,@("wrap", $wrapped)
  }

  $done = $false
  $lastErr = ""
  foreach ($c in $candidates) {
    $r = Try-Run $c[1] $s.Path
    if ($r.Ok) { $done = $true; $modes[$c[0]] = 1 + $modes[$c[0]]; break }
    $lastErr = $r.Err
  }
  if (-not $done) { $fails += ("{0} #{1} -> {2}" -f $s.Page, $s.N, $lastErr) }
}

Write-Output ("compiled OK by mode: " + (($modes.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ", "))
Write-Output "---- not compiling (inspect manually) ----"
if ($fails.Count -eq 0) { Write-Output "none" } else { $fails | ForEach-Object { Write-Output $_ } }

$ErrorActionPreference = "SilentlyContinue"
$root = Get-Location
$outDir = Join-Path $root "wiki_probe\snips"
$exe = Join-Path $root "target\debug\aoxn.exe"
$imp = 'import "../../stdlib/stdlib.ax"'
$stdlibNames = 'gcd|isqrt|is_prime|pow_i|sort\(|min_of|max_of|sum_int|sum_float|binary_search|linear_search|vec_new|vec_push|vec_get|vec_set|vec_free|vec_pop|read_file|write_file|system_exit_code|abs_i|lcm|hypot|clamp_|str_get|buf_new|fill_zero|is_digit|is_alpha|is_space|reverse\('

$fails = @()
foreach ($f in (Get-ChildItem (Join-Path $root "wiki") -Filter *.md | Sort-Object Name)) {
  $text = [System.IO.File]::ReadAllText($f.FullName, [System.Text.Encoding]::UTF8)
  $blocks = [regex]::Matches($text, '(?s)```aoxn\r?\n(.*?)```')
  $i = 0
  foreach ($b in $blocks) {
    $i++
    $code = $b.Groups[1].Value -replace '"\.\./stdlib/', '"../../stdlib/'
    if ($code -notmatch $stdlibNames) { continue }
    if ($code -match '^\s*import') { continue }   # already imported
    $path = Join-Path $outDir "$($f.BaseName)_$i.ax"

    $cands = @()
    if ($code -match 'def main\(') {
      $cands += ,($imp + "`n" + $code)
    } else {
      $lines = $code -split "`n"
      $indented = ($lines | ForEach-Object { if ($_ -match '^\s*$') { "" } else { "    " + $_ } }) -join "`n"
      $cands += ,($imp + "`ndef main() -> int:`n" + $indented + "`n    return 0`n")
    }
    $done = $false
    $err = ""
    foreach ($c in $cands) {
      [System.IO.File]::WriteAllText($path, $c, (New-Object System.Text.UTF8Encoding($false)))
      $out = & $exe run $path 2>&1 | ForEach-Object { "$_" }
      if ($LASTEXITCODE -eq 0) { $done = $true; break } else { $err = (($out | Where-Object { $_ -match '^\[' } | Select-Object -First 1)) }
    }
    if (-not $done) { $fails += ("{0} #{1} -> {2}" -f $f.BaseName, $i, $err) }
  }
}

Write-Output "---- stdlib-context retry failures ----"
if ($fails.Count -eq 0) { Write-Output "none" } else { $fails | ForEach-Object { Write-Output $_ } }

$ErrorActionPreference = "Stop"
$root = Get-Location
$wiki = Join-Path $root "wiki"
$files = Get-ChildItem $wiki -Filter *.md | Sort-Object Name
$problems = @()

# index every real file in the repo (skip build outputs and vendored trees)
$all = @{}
foreach ($p in Get-ChildItem $root -Recurse -File) {
  if ($p.FullName -match '\\(target|\.git|node_modules|\.next)\\\\') { continue }
  $all[$p.FullName.Substring($root.Path.Length + 1).Replace('\', '/')] = $true
}

foreach ($f in $files) {
  $text = [System.IO.File]::ReadAllText($f.FullName, [System.Text.Encoding]::UTF8)

  foreach ($m in [regex]::Matches($text, '\]\((\.\./[^)]+)\)')) {
    $t = $m.Groups[1].Value
    $resolved = Join-Path $wiki $t
    if (-not (Test-Path -LiteralPath $resolved)) { $problems += "DEAD-REPO-LINK: $($f.Name) -> $t" }
  }

  foreach ($m in [regex]::Matches($text, '`((?:src|selfhost|web|examples|tests|stdlib|docs)/[A-Za-z0-9_./-]+)`')) {
    $name = $m.Groups[1].Value
    if ($name.EndsWith('/')) { continue }
    if (-not $all.ContainsKey($name)) {
      # allow a directory path mentioned without a trailing slash
      $isDir = $false
      foreach ($k in $all.Keys) { if ($k.StartsWith($name + '/')) { $isDir = $true; break } }
      if (-not $isDir -and $name -ne 'web/server') { $problems += "UNKNOWN-FILE: $($f.Name) -> $name" }
    }
  }
}

# every fn name cited in backticks must exist as `fn <name>(` somewhere in src/ or tests/
$known = @{}
foreach ($p in (Get-ChildItem (Join-Path $root 'src') -Filter *.rs) + (Get-ChildItem (Join-Path $root 'tests') -Filter *.rs)) {
  $t = [System.IO.File]::ReadAllText($p.FullName, [System.Text.Encoding]::UTF8)
  foreach ($m in [regex]::Matches($t, '(?m)^\s*(?:pub )?fn ([a-z0-9_]+)\(')) { $known[$m.Groups[1].Value] = $true }
}
foreach ($f in $files) {
  $text = [System.IO.File]::ReadAllText($f.FullName, [System.Text.Encoding]::UTF8)
  foreach ($m in [regex]::Matches($text, '`([a-z][a-z0-9_]{6,})`')) {
    $name = $m.Groups[1].Value
    if ($known.ContainsKey($name)) { continue }
    if ($name -match '^(selfhost|stdlib|rejects|import_|optimization_|dependency_|llvm_)') {
      $problems += "UNKNOWN-FN: $($f.Name) -> $name"
    }
  }
}

if ($problems.Count -eq 0) { Write-Output "OK: no problems" } else { $problems | Sort-Object -Unique | ForEach-Object { Write-Output "PROBLEM $_" } }

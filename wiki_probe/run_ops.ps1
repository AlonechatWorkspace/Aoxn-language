$dir = "wiki_probe"
$cases = [ordered]@{}

$cases['op01_bool_eq'] = @'
def main() -> int:
    x = True == False
    y = True and False
    print(x)
    print(y)
    return 0
'@

$cases['op02_float_mod'] = @'
def main() -> int:
    z = 1.0 % 2.0
    print(z)
    return 0
'@

$cases['op03_not_int'] = @'
def main() -> int:
    n = not 5
    print(n)
    return 0
'@

$cases['op04_neg_bool'] = @'
def main() -> int:
    b = -True
    print(b)
    return 0
'@

$cases['op05_str_rep'] = @'
def main() -> int:
    s = "a" * 3
    print(s)
    return 0
'@

$cases['op06_int_div'] = @'
def main() -> int:
    q = 7 / 2
    r = 7 % 2
    print(q)
    print(r)
    return 0
'@

$cases['op07_float_div'] = @'
def main() -> int:
    f = 7.0 / 2.0
    print(f)
    return 0
'@

$cases['op08_void_expr'] = @'
def g():
    print("side effect")

def main() -> int:
    print(g())
    return 0
'@

$cases['op09_len_int'] = @'
def main() -> int:
    print(len(5))
    return 0
'@

$cases['op10_bool_array'] = @'
def main() -> int:
    flags = [True, False, True]
    n = 0
    for f in flags:
        if f:
            n = n + 1
    print(n)
    print(len(flags))
    return 0
'@

$cases['op11_target_os'] = @'
def main() -> int:
    print(target_os())
    print(len(target_os()))
    return 0
'@

$cases['op12_void_fn_ret'] = @'
def nada():
    return

def main() -> int:
    nada()
    return 0
'@

$cases['op13_str_plus_int'] = @'
def main() -> int:
    s = "n=" + 5
    print(s)
    return 0
'@

$cases['op14_assign_field'] = @'
struct P:
    x: int
    y: int

def main() -> int:
    p = P(x=1, y=2)
    q = p
    q.x = 99
    print(p.x)
    print(q.x)
    return 0
'@

$cases['op15_return_void'] = @'
def nope() -> void:
    return

def main() -> int:
    nope()
    return 0
'@

foreach ($k in $cases.Keys) {
  $p = Join-Path (Get-Location) "$dir\$k.ax"
  [System.IO.File]::WriteAllText($p, $cases[$k], (New-Object System.Text.UTF8Encoding($false)))
  Write-Output "=== $k ==="
  & .\target\debug\aoxn.exe run "wiki_probe\$k.ax" 2>&1 | ForEach-Object { "$_" }
}

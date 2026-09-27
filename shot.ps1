$ErrorActionPreference = "Stop"
Start-Process -FilePath "D:\ailanguage\ui_demo.exe"
Start-Sleep -Milliseconds 2000

$p = Get-Process ui_demo -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -eq "Aoxn UI" } | Select-Object -First 1
if ($null -eq $p) { Write-Output "no-window"; exit 1 }
$h = $p.MainWindowHandle

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    public struct RECT { public int L, T, R, B; }
}
"@
[Win]::ShowWindow($h, 9) | Out-Null
[Win]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 800

$r = New-Object Win+RECT
[Win]::GetWindowRect($h, [ref]$r) | Out-Null
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$w = $r.R - $r.L; $hgt = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap($w, $hgt)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, (New-Object System.Drawing.Size($w, $hgt)))
$bmp.Save("D:\ailanguage\ui_shot.png", [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "saved ${w}x${hgt}"

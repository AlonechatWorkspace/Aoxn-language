Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class WEnum {
    public delegate bool EnumCb(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumCb cb, IntPtr l);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    public struct RECT { public int L, T, R, B; }
}
"@
$found = 0
$cb = [WEnum+EnumCb] {
    param($h, $l)
    $sb = New-Object System.Text.StringBuilder 256
    [void][WEnum]::GetWindowText($h, $sb, 256)
    $t = $sb.ToString()
    if ($t -like "*Aoxn*") {
        $r = New-Object WEnum+RECT
        [void][WEnum]::GetWindowRect($h, [ref]$r)
        $pid2 = 0
        [void][WEnum]::GetWindowThreadProcessId($h, [ref]$pid2)
        $vis = [WEnum]::IsWindowVisible($h)
        Write-Output ("title=" + $t + " rect=" + $r.L + "," + $r.T + " " + ($r.R - $r.L) + "x" + ($r.B - $r.T) + " pid=" + $pid2 + " visible=" + $vis)
        $script:found = 1
    }
    return $true
}
[WEnum]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
if ($found -eq 0) { Write-Output "none" }
Get-Process ui_demo -ErrorAction SilentlyContinue | ForEach-Object { Write-Output ("proc pid=" + $_.Id + " mwtitle=" + $_.MainWindowTitle) }

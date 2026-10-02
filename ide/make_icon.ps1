# Generates the Aoxn IDE source icon (1024x1024 PNG). `pnpm tauri icon`
# derives every platform size from it, so this file is the only one that
# needs designing; session helper, not part of the shipped app.
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap $size, $size
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = 'AntiAlias'
$g.TextRenderingHint = 'AntiAliasGridFit'
$g.Clear([System.Drawing.Color]::Transparent)

# rounded-square body in the IDE's editor background, with a lighter
# top-left so it does not read as a flat blob at 16px
$body = New-Object System.Drawing.Drawing2D.GraphicsPath
$r = 180
$body.AddArc(0, 0, $r, $r, 180, 90)
$body.AddArc($size - $r, 0, $r, $r, 270, 90)
$body.AddArc($size - $r, $size - $r, $r, $r, 0, 90)
$body.AddArc(0, $size - $r, $r, $r, 90, 90)
$body.CloseFigure()

$bg = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point 0, 0),
    (New-Object System.Drawing.Point $size, $size),
    [System.Drawing.Color]::FromArgb(255, 45, 45, 46),
    [System.Drawing.Color]::FromArgb(255, 20, 20, 20))
$g.FillPath($bg, $body)

# the accent bar: the same 2px "this tab is active" mark the workbench uses
$accent = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 0, 127, 212))
$g.FillRectangle($accent, 150, 190, 120, 640)

$font = New-Object System.Drawing.Font('Consolas', 380, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
$fmt = New-Object System.Drawing.StringFormat
$fmt.Alignment = 'Center'
$fmt.LineAlignment = 'Center'
$fg = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 236, 236, 236))
$g.DrawString('ax', $font, $fg, (New-Object System.Drawing.RectangleF 150, 90, 730, 840), $fmt)

$out = Join-Path $PSScriptRoot 'icon-source.png'
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "wrote $out"
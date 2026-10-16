# Draws the Loom brand mark (violet→teal rounded square with an "L") as icon.png.
Add-Type -AssemblyName System.Drawing
$s = 512
$bmp = New-Object System.Drawing.Bitmap $s, $s
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = 'AntiAlias'; $g.TextRenderingHint = 'AntiAliasGridFit'
$r = 112; $path = New-Object System.Drawing.Drawing2D.GraphicsPath
$path.AddArc(0, 0, $r, $r, 180, 90); $path.AddArc($s - $r, 0, $r, $r, 270, 90)
$path.AddArc($s - $r, $s - $r, $r, $r, 0, 90); $path.AddArc(0, $s - $r, $r, $r, 90, 90); $path.CloseFigure()
$brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush (New-Object System.Drawing.Point 0, 0), (New-Object System.Drawing.Point $s, $s), ([System.Drawing.Color]::FromArgb(139, 108, 255)), ([System.Drawing.Color]::FromArgb(45, 212, 191))
$g.FillPath($brush, $path)
$font = New-Object System.Drawing.Font 'Segoe UI Semibold', 300, ([System.Drawing.FontStyle]::Bold), ([System.Drawing.GraphicsUnit]::Pixel)
$fmt = New-Object System.Drawing.StringFormat; $fmt.Alignment = 'Center'; $fmt.LineAlignment = 'Center'
$g.DrawString('L', $font, (New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(14, 16, 20))), (New-Object System.Drawing.RectangleF 0, 10, $s, $s), $fmt)
$bmp.Save((Join-Path $PSScriptRoot 'icon.png'), [System.Drawing.Imaging.ImageFormat]::Png)

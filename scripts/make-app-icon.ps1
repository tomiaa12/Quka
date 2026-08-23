Add-Type -AssemblyName System.Drawing

$ErrorActionPreference = "Stop"
$size = 1024
$output = Join-Path $PSScriptRoot "..\src-tauri\icons\app-icon.png"
New-Item -ItemType Directory -Force -Path (Split-Path $output) | Out-Null

$bmp = New-Object System.Drawing.Bitmap $size, $size
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$g.Clear([System.Drawing.Color]::Transparent)

$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$radius = 230
$rect = New-Object System.Drawing.Rectangle 48, 48, 928, 928
$d = $radius * 2
$path.AddArc($rect.X, $rect.Y, $d, $d, 180, 90)
$path.AddArc($rect.Right - $d, $rect.Y, $d, $d, 270, 90)
$path.AddArc($rect.Right - $d, $rect.Bottom - $d, $d, $d, 0, 90)
$path.AddArc($rect.X, $rect.Bottom - $d, $d, $d, 90, 90)
$path.CloseFigure()

$brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush (
    [System.Drawing.Point]::new(80, 40),
    [System.Drawing.Point]::new(940, 980),
    [System.Drawing.Color]::FromArgb(255, 109, 131, 255),
    [System.Drawing.Color]::FromArgb(255, 61, 90, 241)
)
$g.FillPath($brush, $path)

$font = New-Object System.Drawing.Font "Segoe UI", 420, ([System.Drawing.FontStyle]::Bold), ([System.Drawing.GraphicsUnit]::Pixel)
$format = New-Object System.Drawing.StringFormat
$format.Alignment = [System.Drawing.StringAlignment]::Center
$format.LineAlignment = [System.Drawing.StringAlignment]::Center
$g.DrawString("Q", $font, [System.Drawing.Brushes]::White, (New-Object System.Drawing.RectangleF 0, 36, $size, $size), $format)

$bmp.Save($output, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()
$brush.Dispose()
$path.Dispose()
$font.Dispose()
Write-Host "wrote $output"

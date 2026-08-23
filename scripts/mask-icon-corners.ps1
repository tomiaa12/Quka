# Make pixels outside the rounded-square icon fully transparent.
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing, System.Drawing.Primitives -TypeDefinition @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class IconCornerMask {
    public static void Apply(string input, string output, float radiusRatio) {
        using (var src = new Bitmap(input)) {
            int size = 1024;
            using (var dst = new Bitmap(size, size, PixelFormat.Format32bppArgb))
            using (var g = Graphics.FromImage(dst)) {
                g.Clear(Color.Transparent);
                g.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic;
                g.PixelOffsetMode = System.Drawing.Drawing2D.PixelOffsetMode.HighQuality;
                g.DrawImage(src, new Rectangle(0, 0, size, size));

                var data = dst.LockBits(
                    new Rectangle(0, 0, size, size),
                    ImageLockMode.ReadWrite,
                    PixelFormat.Format32bppArgb
                );
                int bytes = Math.Abs(data.Stride) * size;
                byte[] px = new byte[bytes];
                Marshal.Copy(data.Scan0, px, 0, bytes);

                float radius = size * radiusRatio;
                Color bg = Color.FromArgb(px[2], px[1], px[0]);

                for (int y = 0; y < size; y++) {
                    int row = y * data.Stride;
                    for (int x = 0; x < size; x++) {
                        int i = row + x * 4;
                        byte b = px[i], gv = px[i + 1], r = px[i + 2];

                        float cover = Cover(x, y, size, radius);
                        if (IsBackdrop(r, gv, b, bg)) {
                            cover = 0f;
                        }

                        if (cover <= 0.004f) {
                            px[i] = 0; px[i + 1] = 0; px[i + 2] = 0; px[i + 3] = 0;
                            continue;
                        }

                        if (cover < 0.996f) {
                            px[i + 3] = (byte)Math.Max(0, Math.Min(255, (int)Math.Round(255f * cover)));
                        }
                    }
                }

                Marshal.Copy(px, 0, data.Scan0, bytes);
                dst.UnlockBits(data);
                dst.Save(output, ImageFormat.Png);
            }
        }
    }

    static bool IsBackdrop(byte r, byte g, byte b, Color bg) {
        int dr = Math.Abs(r - bg.R);
        int dg = Math.Abs(g - bg.G);
        int db = Math.Abs(b - bg.B);
        bool grayish = Math.Abs(r - g) < 18 && Math.Abs(g - b) < 18;
        return (dr < 28 && dg < 28 && db < 28 && grayish && r > 80 && r < 220);
    }

    static float Cover(int x, int y, int size, float radius) {
        float cx = (x < radius) ? radius - x
            : (x > size - 1 - radius) ? x - (size - 1 - radius)
            : 0f;
        float cy = (y < radius) ? radius - y
            : (y > size - 1 - radius) ? y - (size - 1 - radius)
            : 0f;
        if (cx == 0f || cy == 0f) return 1f;
        float d = (float)Math.Sqrt(cx * cx + cy * cy);
        float aa = 1.25f;
        if (d <= radius - aa) return 1f;
        if (d >= radius + aa) return 0f;
        return (radius + aa - d) / (2f * aa);
    }
}
"@

$ErrorActionPreference = "Stop"
$icons = Join-Path $PSScriptRoot "..\src-tauri\icons"
$source = Join-Path $icons "app-icon.png"
if (-not (Test-Path $source)) {
    $source = Join-Path $icons "quickly-app-icon.png"
}
$output = Join-Path $icons "icon-source.png"
[IconCornerMask]::Apply($source, $output, 0.2237)
Write-Host "wrote $output"

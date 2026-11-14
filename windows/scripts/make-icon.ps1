<#
.SYNOPSIS
Draws the LingoSnap seal (a vermilion rounded square with the white glyph 译)
and packs it into windows\assets\lingosnap.ico.

.DESCRIPTION
The .ico is committed, so builds do not need this script. Run it only to change
the artwork. Sizes 16 and 20 px are too small for the glyph and use a solid seal
with a white bar. Every entry is stored as PNG.
#>
[CmdletBinding()]
param(
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
if (-not $OutputPath) {
    $scriptDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
    $OutputPath = Join-Path $scriptDirectory '..\assets\lingosnap.ico'
}
Add-Type -AssemblyName System.Drawing

$sizes = 16, 20, 24, 32, 40, 48, 64, 256
$seal = [System.Drawing.ColorTranslator]::FromHtml('#B23A25')
$glyph = [string][char]0x8BD1

function New-RoundedPath([single]$size, [single]$radius) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $radius * 2
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc($size - $d, 0, $d, $d, 270, 90)
    $path.AddArc($size - $d, $size - $d, $d, $d, 0, 90)
    $path.AddArc(0, $size - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    return $path
}

function New-SealPng([int]$size) {
    $bitmap = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $g.SmoothingMode = 'AntiAlias'
        $g.TextRenderingHint = 'AntiAliasGridFit'
        $g.Clear([System.Drawing.Color]::Transparent)
        # The page's .seal is 4px radius at 27px.
        $radius = [Math]::Max(2.0, $size * 4.0 / 27.0)
        $path = New-RoundedPath $size $radius
        $g.FillPath((New-Object System.Drawing.SolidBrush($seal)), $path)
        $white = [System.Drawing.Brushes]::White
        if ($size -lt 24) {
            $barWidth = [Math]::Round($size * 0.5)
            $barHeight = [Math]::Max(2, [Math]::Round($size * 0.14))
            $g.FillRectangle($white, [single](($size - $barWidth) / 2), [single](($size - $barHeight) / 2), [single]$barWidth, [single]$barHeight)
        }
        else {
            $font = New-Object System.Drawing.Font('Microsoft YaHei', [single]($size * 0.62), [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
            $format = New-Object System.Drawing.StringFormat
            $format.Alignment = 'Center'
            $format.LineAlignment = 'Center'
            $box = New-Object System.Drawing.RectangleF(0, [single]($size * 0.02), $size, $size)
            $g.DrawString($glyph, $font, $white, $box, $format)
        }
    }
    finally {
        $g.Dispose()
    }
    $stream = New-Object System.IO.MemoryStream
    $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
    return , $stream.ToArray()
}

$images = foreach ($size in $sizes) { , (New-SealPng $size) }

$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($out)
$w.Write([uint16]0)               # reserved
$w.Write([uint16]1)               # type: icon
$w.Write([uint16]$sizes.Count)    # image count
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dimension = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([byte]$dimension)    # width (0 = 256)
    $w.Write([byte]$dimension)    # height
    $w.Write([byte]0)             # palette colours
    $w.Write([byte]0)             # reserved
    $w.Write([uint16]1)           # colour planes
    $w.Write([uint16]32)          # bits per pixel
    $w.Write([uint32]$images[$i].Length)
    $w.Write([uint32]$offset)
    $offset += $images[$i].Length
}
foreach ($image in $images) { $w.Write($image) }
$w.Flush()

$target = [System.IO.Path]::GetFullPath($OutputPath)
New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($target)) | Out-Null
[System.IO.File]::WriteAllBytes($target, $out.ToArray())
Write-Host "Wrote $target ($($out.Length) bytes, sizes: $($sizes -join ', '))"

# Optional preview strip next to the icon for review.
$previewPath = [System.IO.Path]::ChangeExtension($target, 'preview.png')
$stripWidth = [int](($sizes | Measure-Object -Sum).Sum + 8 * ($sizes.Count + 1))
$strip = New-Object System.Drawing.Bitmap($stripWidth, 280)
$sg = [System.Drawing.Graphics]::FromImage($strip)
$sg.Clear([System.Drawing.ColorTranslator]::FromHtml('#F7F3EB'))
$x = 8
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $image = [System.Drawing.Image]::FromStream((New-Object System.IO.MemoryStream(, $images[$i])))
    $sg.DrawImage($image, $x, 8, $sizes[$i], $sizes[$i])
    $x += $sizes[$i] + 8
}
$sg.Dispose()
$strip.Save($previewPath, [System.Drawing.Imaging.ImageFormat]::Png)
$strip.Dispose()
Write-Host "Preview: $previewPath"

Add-Type -AssemblyName System.Drawing

$out = Join-Path (Split-Path $PSScriptRoot -Parent) "src-tauri\icons"
New-Item -ItemType Directory -Force $out | Out-Null

function New-BernardBitmap([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $rect = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
    $radius = [int]($size * 0.24)
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $radius * 2
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc($size - $d - 1, 0, $d, $d, 270, 90)
    $path.AddArc($size - $d - 1, $size - $d - 1, $d, $d, 0, 90)
    $path.AddArc(0, $size - $d - 1, $d, $d, 90, 90)
    $path.CloseFigure()

    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        $rect,
        [System.Drawing.Color]::FromArgb(255, 28, 30, 34),
        [System.Drawing.Color]::FromArgb(255, 17, 18, 21),
        45.0)
    $g.FillPath($brush, $path)
    $brush.Dispose()

    $ringPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(90, 240, 226, 200), [Math]::Max(1.0, $size * 0.018))
    $inset = [int]($size * 0.055)
    $insetRect = [System.Drawing.Rectangle]::new($inset, $inset, $size - $inset * 2, $size - $inset * 2)
    $insetPath = New-Object System.Drawing.Drawing2D.GraphicsPath
    $ir = [int]($radius * 0.8)
    $id = $ir * 2
    $insetPath.AddArc($insetRect.X, $insetRect.Y, $id, $id, 180, 90)
    $insetPath.AddArc($insetRect.Right - $id, $insetRect.Y, $id, $id, 270, 90)
    $insetPath.AddArc($insetRect.Right - $id, $insetRect.Bottom - $id, $id, $id, 0, 90)
    $insetPath.AddArc($insetRect.X, $insetRect.Bottom - $id, $id, $id, 90, 90)
    $insetPath.CloseFigure()
    $g.DrawPath($ringPen, $insetPath)
    $ringPen.Dispose()
    $insetPath.Dispose()

    $face = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 242, 231, 211))
    $eye = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 22, 23, 27))
    $eyeW = [Math]::Max(2.0, $size * 0.075)
    $eyeH = [Math]::Max(2.0, $size * 0.105)
    $g.FillEllipse($face, $size * 0.20, $size * 0.20, $size * 0.60, $size * 0.60)
    $g.FillEllipse($eye, $size * 0.335, $size * 0.355, $size * 0.075, $size * 0.105)
    $g.FillEllipse($eye, $size * 0.590, $size * 0.355, $size * 0.075, $size * 0.105)
    $mouth = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 26, 27, 31), [Math]::Max(1.0, $size * 0.028))
    $mouth.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $mouth.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.DrawArc($mouth, $size * 0.38, $size * 0.50, $size * 0.24, $size * 0.16, 20, 140)
    $mouth.Dispose()
    $face.Dispose()
    $eye.Dispose()
    $path.Dispose()
    $g.Dispose()
    return $bmp
}

$source = New-BernardBitmap 512
$source.Save((Join-Path $out "icon.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$source.Dispose()

function Save-Resized([int]$size, [string]$name) {
    $bmp = New-BernardBitmap $size
    $bmp.Save((Join-Path $out $name), [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

Save-Resized 32 "32x32.png"
Save-Resized 128 "128x128.png"
Save-Resized 256 "128x128@2x.png"
Save-Resized 256 "icon-256.png"

# Windows ICO containing a 256x256 PNG frame.
$png = [System.IO.File]::ReadAllBytes((Join-Path $out "icon-256.png"))
$stream = New-Object System.IO.MemoryStream
$writer = New-Object System.IO.BinaryWriter($stream)
$writer.Write([UInt16]0)
$writer.Write([UInt16]1)
$writer.Write([UInt16]1)
$writer.Write([Byte]0)
$writer.Write([Byte]0)
$writer.Write([Byte]0)
$writer.Write([Byte]0)
$writer.Write([UInt16]1)
$writer.Write([UInt16]32)
$writer.Write([UInt32]$png.Length)
$writer.Write([UInt32]22)
$writer.Write($png)
$writer.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $out "icon.ico"), $stream.ToArray())
$writer.Dispose()
$stream.Dispose()
Remove-Item (Join-Path $out "icon-256.png")

Get-ChildItem $out | Select-Object Name, Length

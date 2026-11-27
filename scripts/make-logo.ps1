# Draws assets\logo.svg's shapes to PNG files (assets\logo-256.png, assets\logo-32.png):
# the engine and the launcher show them as the windows' icon. Run it again after a
# change of the logo.   scripts\make-logo.ps1
Add-Type -AssemblyName System.Drawing
$assets = Join-Path (Split-Path $PSScriptRoot) "assets"
$colour = { param($hex, $alpha = 255) $c = [Drawing.ColorTranslator]::FromHtml($hex); [Drawing.Color]::FromArgb($alpha, $c.R, $c.G, $c.B) }
function Rounded([single]$x, [single]$y, [single]$w, [single]$h, [single]$r) {
    $p = New-Object Drawing.Drawing2D.GraphicsPath; $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90); $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90); $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90); $p.CloseFigure(); $p
}
foreach ($size in 256, 32) {
    $bmp = New-Object Drawing.Bitmap $size, $size
    $g = [Drawing.Graphics]::FromImage($bmp); $g.SmoothingMode = 'AntiAlias'; $g.PixelOffsetMode = 'HighQuality'
    $g.ScaleTransform($size / 512, $size / 512)
    # chip pins (left out of the small one: they turn to noise)
    if ($size -ge 64) {
        $pin = New-Object Drawing.SolidBrush (& $colour '#7c5cff' 140)
        foreach ($at in 156, 216, 276, 336) {
            foreach ($r in @($at, 24, 20, 48), @($at, 440, 20, 48), @(24, $at, 48, 20), @(440, $at, 48, 20)) { $p = Rounded $r[0] $r[1] $r[2] $r[3] 4; $g.FillPath($pin, $p) }
        }
    }
    # chip body
    $body = Rounded 72 72 368 368 56
    $bg = New-Object Drawing.Drawing2D.LinearGradientBrush ([Drawing.PointF]::new(72, 72)), ([Drawing.PointF]::new(440, 440)), (& $colour '#151926'), (& $colour '#0b0d12')
    $g.FillPath($bg, $body); $g.DrawPath((New-Object Drawing.Pen (& $colour '#7c5cff'), 10), $body)
    # speed lines
    $line = New-Object Drawing.Pen (& $colour '#3ddc84' 230), 14; $line.StartCap = 'Round'; $line.EndCap = 'Round'
    $g.DrawLine($line, 104, 190, 140, 190); $g.DrawLine($line, 96, 236, 156, 236); $g.DrawLine($line, 108, 282, 172, 282)
    # the V
    $v = New-Object Drawing.Drawing2D.LinearGradientBrush ([Drawing.PointF]::new(150, 118)), ([Drawing.PointF]::new(362, 394)), (& $colour '#3ddc84'), (& $colour '#7c5cff')
    $pen = New-Object Drawing.Pen $v, 52; $pen.StartCap = 'Round'; $pen.EndCap = 'Round'; $pen.LineJoin = 'Round'
    $g.DrawLines($pen, [Drawing.PointF[]]@([Drawing.PointF]::new(176, 144), [Drawing.PointF]::new(256, 368), [Drawing.PointF]::new(336, 144)))
    # arrow head pointing right, to Windows
    $arrow = New-Object Drawing.Pen (& $colour '#e6ecf5'), 12; $arrow.StartCap = 'Round'; $arrow.EndCap = 'Round'; $arrow.LineJoin = 'Round'
    $g.DrawLine($arrow, 352, 236, 396, 236)
    $g.DrawLines($arrow, [Drawing.PointF[]]@([Drawing.PointF]::new(378, 216), [Drawing.PointF]::new(398, 236), [Drawing.PointF]::new(378, 256)))
    $g.Dispose(); $bmp.Save((Join-Path $assets "logo-$size.png"), [Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
}
"made logo-256.png and logo-32.png in $assets"

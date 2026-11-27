# Renders the app icon (our own wheel drawing, no MOZA artwork) and writes
#   assets\icon.ico       multi-size icon embedded in the exe (build.rs)
#   assets\icon-256.png   window/taskbar icon loaded at runtime
# Re-run after changing the design:
#   powershell -ExecutionPolicy Bypass -File tools\make_icon.ps1
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root   = Split-Path $PSScriptRoot -Parent
$assets = Join-Path $root 'assets'
New-Item -ItemType Directory -Force $assets | Out-Null

function C([string]$hex) { [System.Drawing.ColorTranslator]::FromHtml($hex) }

# Colours follow src/gui/theme.rs.
$bg     = C '#16191C'
$rim    = C '#AEB7C0'
$hub    = C '#3A424A'
$accent = C '#A3E635'

function New-RoundedRect([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
  $p = New-Object System.Drawing.Drawing2D.GraphicsPath
  $d = 2 * $r
  $p.AddArc($x, $y, $d, $d, 180, 90)
  $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
  $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
  $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
  $p.CloseFigure()
  return $p
}

# Draws the icon at $size px. All geometry is in a 256-unit design space.
function Render([int]$size) {
  $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode   = 'AntiAlias'
  $g.PixelOffsetMode = 'HighQuality'
  $g.Clear([System.Drawing.Color]::Transparent)
  $g.ScaleTransform($size / 256.0, $size / 256.0)

  # Rounded dark tile.
  $tile = New-RoundedRect 8 8 240 240 52
  $g.FillPath((New-Object System.Drawing.SolidBrush $bg), $tile)

  $hubBrush = New-Object System.Drawing.SolidBrush $hub
  # Spokes: left, right, bottom. They run under the rim.
  $pts = @(
    @(46, 112), @(128, 104), @(210, 112), @(206, 146), @(160, 150),
    @(150, 214), @(106, 214), @(96, 150), @(50, 146)
  ) | ForEach-Object { New-Object System.Drawing.PointF $_[0], $_[1] }
  $g.FillPolygon($hubBrush, [System.Drawing.PointF[]]$pts)

  # Rim ring: outer radius 104, inner 78. Thicker at small sizes so it survives.
  $rimW = if ($size -le 32) { 34 } else { 26 }
  $rimR = 104 - $rimW / 2
  $pen = New-Object System.Drawing.Pen $rim, $rimW
  $g.DrawEllipse($pen, 128 - $rimR, 128 - $rimR, 2 * $rimR, 2 * $rimR)

  # Top-centre marker in the accent colour.
  $markW = if ($size -le 32) { 26 } else { 18 }
  $g.FillRectangle((New-Object System.Drawing.SolidBrush $accent), 128 - $markW / 2, 128 - 104 - 1, $markW, $rimW + 2)

  # Centre badge: accent hexagon.
  $hexR = if ($size -le 32) { 30 } else { 24 }
  $hex = 0..5 | ForEach-Object {
    $a = [Math]::PI / 3 * $_
    New-Object System.Drawing.PointF (128 + $hexR * [Math]::Cos($a)), (128 + $hexR * [Math]::Sin($a))
  }
  $g.FillPolygon((New-Object System.Drawing.SolidBrush $accent), [System.Drawing.PointF[]]$hex)

  $g.Dispose()
  return $bmp
}

function PngBytes($bmp) {
  $ms = New-Object System.IO.MemoryStream
  $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
  return ,$ms.ToArray()
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$images = foreach ($s in $sizes) {
  $b = Render $s
  $bytes = PngBytes $b
  if ($s -eq 256) { [System.IO.File]::WriteAllBytes((Join-Path $assets 'icon-256.png'), $bytes) }
  $b.Dispose()
  [pscustomobject]@{ Size = $s; Bytes = $bytes }
}

# ICO container with PNG-compressed entries.
$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $out
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$images.Count)
$offset = 6 + 16 * $images.Count
foreach ($i in $images) {
  $dim = if ($i.Size -ge 256) { 0 } else { $i.Size }
  $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
  $w.Write([uint16]1); $w.Write([uint16]32)
  $w.Write([uint32]$i.Bytes.Length); $w.Write([uint32]$offset)
  $offset += $i.Bytes.Length
}
foreach ($i in $images) { $w.Write($i.Bytes) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $assets 'icon.ico'), $out.ToArray())

Get-ChildItem $assets | Format-Table Name, Length -AutoSize

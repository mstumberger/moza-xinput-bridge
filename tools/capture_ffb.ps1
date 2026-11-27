# Guided USBPcap capture of a MOZA wheelbase, from plug-in through in-game FFB.
# Run elevated: powershell -ExecutionPolicy Bypass -File tools\capture_ffb.ps1
# Captures every root hub to captures\ (gitignored) and writes phase markers.
$dir   = Join-Path (Split-Path $PSScriptRoot -Parent) 'captures'
New-Item -ItemType Directory -Force $dir | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd_HHmmss'
$exe   = 'C:\Program Files\USBPcap\USBPcapCMD.exe'
$mark  = Join-Path $dir "markers_$stamp.txt"

function Start-Caps {
  $ps = @()
  foreach ($n in 1, 2, 3) {
    $dev = '\\.\USBPcap' + $n
    $out = Join-Path $dir ("ffb_{0}_hub{1}.pcap" -f $stamp, $n)
    $ps += Start-Process -FilePath $exe -ArgumentList '-d', $dev, '-o', $out, '-A', '--inject-descriptors' -PassThru -WindowStyle Hidden
  }
  Start-Sleep 3
  return , @($ps | Where-Object { -not $_.HasExited })
}

$procs = Start-Caps
if ($procs.Count -eq 0) {
  Write-Host 'USBPcap filter not attached - restarting USB root hubs (keyboard/mouse may blink)...' -ForegroundColor Yellow
  Get-PnpDevice -PresentOnly | Where-Object { $_.FriendlyName -match 'Root Hub' } | ForEach-Object {
    Write-Host "  restart $($_.InstanceId)"
    pnputil /restart-device "$($_.InstanceId)" | Out-Null
  }
  Start-Sleep 6
  $procs = Start-Caps
}
if ($procs.Count -eq 0) {
  Write-Host 'Still no capture device. Interactive USBPcap console follows:' -ForegroundColor Red
  Write-Host '  answer y if it asks to restart USB devices, pick the hub with the MOZA base (was 2),'
  Write-Host '  Enter for all devices, output file:'
  Write-Host ("  {0}\ffb_{1}_manual.pcap" -f $dir, $stamp) -ForegroundColor Cyan
  Write-Host '  then do: plug in -> Pit House -> game -> turn wheel -> drive -> Ctrl+C'
  & $exe
  Read-Host 'Enter to close'
  exit 1
}

$t0 = Get-Date
"start $($t0.ToString('o'))" | Out-File $mark -Encoding utf8
function Mark($label) {
  $s = ((Get-Date) - $t0).TotalSeconds
  ("{0:N2}s {1}" -f $s, $label) | Tee-Object -FilePath $mark -Append | Write-Host -ForegroundColor Green
}

Write-Host "`nCAPTURING on $($procs.Count) hub(s). Press Enter after each step:`n" -ForegroundColor Yellow
$steps = @(
  'PLUG the wheelbase USB in now, wait ~10 s until buttons/LEDs are alive',
  'START Pit House, wait until it shows the base connected',
  'START the game and get on track (car stationary)',
  'Car STATIONARY: turn the wheel slowly full LEFT then full RIGHT (feel the centering force)',
  'DRIVE ~20 s: a few corners, curbs, maybe tap a wall',
  'STOP the car, LET GO of the wheel for 5 s',
  'QUIT the game (Pit House can stay open) - this ends the capture'
)
$i = 1
foreach ($s in $steps) {
  Read-Host "[$i/$($steps.Count)] $s  -> Enter when done" | Out-Null
  Mark "done: $s"
  $i++
}

$procs | ForEach-Object { if (-not $_.HasExited) { Stop-Process -Id $_.Id -Force } }
Start-Sleep 1
Get-ChildItem $dir -Filter "ffb_${stamp}_*.pcap" | Format-Table Name, Length -AutoSize
Write-Host "Markers: $mark" -ForegroundColor Cyan
Read-Host 'Done. Enter to close'

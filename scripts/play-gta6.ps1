# Plays GTA VI with Vilocity (in its sandbox) and opens a live monitor window.
#   scripts\play-gta6.ps1 [-Game <folder>] [-Defaults]
# -Defaults leaves the frame-rate switches below off (the fork's defaults).
param(
    [string]$Game = $env:VILOCITY_GTA6_DIR,
    [switch]$Defaults
)
if (-not $Game) { throw "Pass -Game <game folder> or set VILOCITY_GTA6_DIR" }
$root = Split-Path $PSScriptRoot
$exe = Join-Path $root "target\release\vilocity.exe"
$logs = Join-Path $env:LOCALAPPDATA "vilocity\logs"
New-Item -ItemType Directory -Force $logs | Out-Null
$log = Join-Path $logs ("gta6-{0:yyyyMMdd-HHmmss}.log" -f (Get-Date))

if (-not $Defaults) {
    # The fork's measured set (6-7 fps where the defaults gave 4.5-5 on an
    # RTX 4070); its MERGE_GAP is left out because it is not exact.
    $env:VILOCITY_ASYNC_SUBMIT = "1"     # submissions sent by a thread of our own
    $env:VILOCITY_QUEUE2 = "1"           # read-backs on a copy queue
    $env:VILOCITY_INDIRECT_WINDOW = "1"  # cheaper indirect draws
    $env:VILOCITY_TEXTURE_NO_WAIT = "1"  # texture refreshes do not wait for remaps
    # Ours, from the run logs on an RTX 5080: the texture pass took up to
    # 250 ms a frame (750 with a backlog); 100 ms keeps frames shorter while
    # textures stream in over more frames.
    $env:VILOCITY_TEXTURE_STAGE_MS = "100"
    # 4.7 GB of the card's copies spilled into PC memory; levels the title
    # does not hold take no card memory with this (the fork: 2-3 GB saved).
    $env:VILOCITY_LEVELS_ON_DEMAND = "1"
    # VItoriaBr's preset (RTX 5060 Ti, 3.5-6 fps in the world, no driver
    # timeouts): a 1080p television instead of 4K, so the title renders
    # less; wider refreshes and merged copies; more card memory for buffers.
    $env:VILOCITY_DISPLAY_MODE = "1920x1080"
    $env:VILOCITY_WIDE_REFRESH = "1"
    $env:VILOCITY_WIDE_MERGE = "1"
    $env:VILOCITY_JOIN_COPIES = "1"
    $env:VILOCITY_CARD_BUFFERS_MB = "6144"
}

# One quoted string: the paths hold spaces ("Vilocity project").
$monitor = Join-Path $PSScriptRoot "monitor-gta6.ps1"
Start-Process powershell -ArgumentList "-NoExit -NoProfile -ExecutionPolicy Bypass -File `"$monitor`" -Log `"$log`""
Write-Host "log: $log"
& $exe play $Game *> $log
Write-Host "Vilocity exited with code $LASTEXITCODE (log: $log)"

# Plays GTA VI with Vilocity (in its sandbox) and opens a live monitor window.
#   scripts\play-gta6.ps1 [-Game <folder>] [-Defaults] [-Diag] [-NoSandbox]
# -Defaults leaves the frame-rate switches below off (the fork's defaults).
# -Diag runs the diagnostics build (cargo build --profile diag -p vilocity
# --features diagnostics), which logs where each frame's time goes.
# -NoSandbox runs the game as you, outside its sandbox (for measuring).
param(
    [string]$Game = $env:VILOCITY_GTA6_DIR,
    [switch]$Defaults,
    [switch]$Diag,
    [switch]$NoSandbox
)
if (-not $Game) { throw "Pass -Game <game folder> or set VILOCITY_GTA6_DIR" }
$root = Split-Path $PSScriptRoot
$exe = Join-Path $root $(if ($Diag) { "target\diag\vilocity.exe" } else { "target\release\vilocity.exe" })
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
    $env:VILOCITY_COPY_PLAIN = "1"       # title memory copied in place (fork 0b0074e: 145 -> 124 ms at 4,500 draws)
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
    # Ray passes trace a structure that is never built and write garbage:
    # rainbow speckles, black walls (list from puffnfresh/rebox's fast.c;
    # 2026-11-25 runs s15/s16: walls lit, fewer speckles without them).
    # CS_CombinedReflections reads what they leave in their buffers (left out,
    # their producers leave old garbage there): without it, run s21, faces
    # and walls are clean and the rainbow glitter is mostly gone.
    # CS_SpatialFilter* and CS_BilateralBlur* denoise those ray results: 90 of
    # 203 ms of card time a frame (timing run gta6-20261125-155709); without
    # them the frame went from 300-410 to 207 ms and the glitter they spread
    # over heads is gone (run gta6-20261125-160106).
    if (-not $env:VILOCITY_LEAVE_OUT) {
        $env:VILOCITY_LEAVE_OUT = "TraceRays_BVH,RTShadowTrace,RTDistantShadowTrace,RTDistantLODLightShadow,TraceShadowRays,RaytraceShadows,RaytraceVBuffer,IrradianceCache_ShadowTrace,ShadowRaytracing,CollisionRaytraced,PPR_Raytrace,CS_SpatialFilter,CS_BilateralBlur"
    }
    # Left out, CS_CombinedReflections's outputs kept whatever their memory
    # held (glitter on some runs, none on others): cleared, the lighting
    # reads no reflected light there instead.
    if (-not $env:VILOCITY_ZERO_OUT) { $env:VILOCITY_ZERO_OUT = "CS_CombinedReflections" }
}

# One quoted string: the paths hold spaces ("Vilocity project").
$monitor = Join-Path $PSScriptRoot "monitor-gta6.ps1"
# AMD's PC FidelityFX upscaler (FSR), if downloaded into this folder, runs
# the title's upscaling natively instead of the console build translated.
$ffx = Join-Path $env:LOCALAPPDATA "vilocity\fidelityfx"
if (Test-Path (Join-Path $ffx "amd_fidelityfx_upscaler_dx12.dll")) { $env:VILOCITY_FIDELITYFX_DIR = $ffx }

$watch = Start-Process powershell -PassThru -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$monitor`" -Log `"$log`""
Write-Host "log: $log"
try {
    $sandbox = @(if ($NoSandbox) { '--no-sandbox' })
    & $exe play $Game @sandbox *> $log
    Write-Host "Vilocity exited with code $LASTEXITCODE (log: $log)"
} finally {
    # The monitor goes with the game (also when this window is closed with Ctrl+C).
    if (-not $watch.HasExited) { Stop-Process -Id $watch.Id -Force -ErrorAction SilentlyContinue }
}

# Plays GTA VI with Vilocity (in its sandbox) and opens a live monitor window.
#   scripts\play-gta6.ps1 [-Game <folder>] [-Defaults] [-Diag] [-NoSandbox] [-SeriesX]
# -Defaults leaves the frame-rate switches below off (the fork's defaults).
# -Diag runs the diagnostics build (cargo build --profile diag -p vilocity
# --features diagnostics), which logs where each frame's time goes.
# -NoSandbox runs the game as you, outside its sandbox (for measuring).
# -SeriesX shows the title a Series X instead of the default Series S.
param(
    [string]$Game = $env:VILOCITY_GTA6_DIR,
    [switch]$Defaults,
    [switch]$Diag,
    [switch]$NoSandbox,
    [switch]$SeriesX
)
if (-not $Game) { throw "Pass -Game <game folder> or set VILOCITY_GTA6_DIR" }
# The switches below are set in this window's environment: kept here and put
# back when the game exits, so a later run (with -Defaults, say) starts clean.
$before = @{}
Get-ChildItem Env: | Where-Object Name -like 'VILOCITY_*' | ForEach-Object { $before[$_.Name] = $_.Value }
$root = Split-Path $PSScriptRoot
# The title picks its renderer by console model. As a Series S it renders
# fewer pixels and leaves out the ray-traced and hair-strand passes this
# layer cannot run: 11 fps (90 ms a frame, the card no longer the limit)
# against 3 fps as a Series X, with lit rooms, the inmate's hair and the
# jumpsuits' orange (run gta6-20261126-001004). Still missing on Series S:
# the "INMATE" print on the jumpsuit's back.
if (-not $env:VILOCITY_CONSOLE) { $env:VILOCITY_CONSOLE = if ($SeriesX) { "series-x" } else { "series-s" } }
# Series S at half internal resolution, upscaled by the title's own upscaler
# (trustno1-coder/VIctory-NVIDIA, how the 25-32 fps NVIDIA reports run):
# 84-90 ms against 95-117 ms a frame, picture good (2026-11-26).
if ($env:VILOCITY_CONSOLE -eq "series-s") {
    if (-not $env:VILOCITY_RENDER_SCALE) { $env:VILOCITY_RENDER_SCALE = "0.5" }
    if (-not $env:VILOCITY_OWN_UPSCALER) { $env:VILOCITY_OWN_UPSCALER = "1" }
}
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
    # Texture refreshes wait for title memory to hold still (0). With 1 a refresh
    # that met a remap read nothing and the texture kept zeros for the run: Lucia's
    # hair a flat dark cap, neck black, no "INMATE" print in about 2 launches of 3
    # (fxceless/victory-fork #923; hair in 4 of 4 cell captures with 0, same fps).
    if (-not $env:VILOCITY_TEXTURE_NO_WAIT) { $env:VILOCITY_TEXTURE_NO_WAIT = "2" }
    $env:VILOCITY_COPY_PLAIN = "1"       # title memory copied in place (fork 0b0074e: 145 -> 124 ms at 4,500 draws)
    # 4.7 GB of the card's copies spilled into PC memory; levels the title
    # does not hold take no card memory with this (the fork: 2-3 GB saved).
    $env:VILOCITY_LEVELS_ON_DEMAND = "1"
    # VItoriaBr's preset (RTX 5060 Ti, 3.5-6 fps in the world, no driver
    # timeouts): a 1080p television instead of 4K, so the title renders
    # less; wider refreshes and merged copies; more card memory for buffers.
    if (-not $env:VILOCITY_DISPLAY_MODE) { $env:VILOCITY_DISPLAY_MODE = "1920x1080" }
    $env:VILOCITY_WIDE_REFRESH = "1"
    $env:VILOCITY_WIDE_MERGE = "1"
    $env:VILOCITY_JOIN_COPIES = "1"
    # Room on the card kept for copies of title buffers counts as used when
    # the texture queue asks whether the card has room: 6,144 MB kept, of
    # which some 660 MB are ever made, held 16,000 textures back while the
    # card was half empty (the "INMATE" print, faces in the cutscenes).
    # 1,536 MB: all of them shown, 11 fps (run gta6-20261126-015721).
    if (-not $env:VILOCITY_CARD_BUFFERS_MB) { $env:VILOCITY_CARD_BUFFERS_MB = "2048" }
    # Title buffers from 64 KiB up get card copies too, compared on the card
    # (fxceless/victory-fork c36971b; trustno1-coder/VIctory-NVIDIA uses 64 / 2048):
    # copying back 20-29 -> 14-18 ms a frame, picture unchanged (2026-11-26).
    if (-not $env:VILOCITY_CARD_BUFFERS_MIN_KB) { $env:VILOCITY_CARD_BUFFERS_MIN_KB = "64" }
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
    # CS_ScreenProbes_EvaluateRayRequests*, CS_SortRayTiles and
    # CS_Primary_RegisterAllocationRequests only prepare work for the ray
    # passes above: some 10 ms of card time a frame (timing run
    # gta6-20261125-234649); the picture the same without them (run
    # gta6-20261125-235928).
    if (-not $env:VILOCITY_LEAVE_OUT) {
        $env:VILOCITY_LEAVE_OUT = "TraceRays_BVH,RTShadowTrace,RTDistantShadowTrace,RTDistantLODLightShadow,TraceShadowRays,RaytraceShadows,RaytraceVBuffer,IrradianceCache_ShadowTrace,ShadowRaytracing,CollisionRaytraced,PPR_Raytrace,CS_SpatialFilter,CS_BilateralBlur,CS_ScreenProbes_EvaluateRayRequests,CS_SortRayTiles,CS_Primary_RegisterAllocationRequests"
    }
    # Left out, CS_CombinedReflections's outputs kept whatever their memory
    # held (glitter on some runs, none on others): cleared, the lighting
    # reads no reflected light there instead. CS_ScreenProbes_Inject put the
    # rays' stale buffers into the probes' radiance, which the final gather
    # turned into rainbow glitter on faces and jackets and a red tint on the
    # orange jumpsuits (target dump, run gta6-20261125-173003): cleared, and
    # its radiance given the ambient light (crate::ambient::INJECT).
    # CS_DefragHeap_ComputeRelocations, the title's defragmenter of card
    # memory, took 230 ms of card time a frame in some launches (timing run
    # gta6-20261126-011801, against 0 in the others): those launches ran at
    # 3-5 fps with black faces, a bald dark head and red jumpsuits. Cleared,
    # it plans no moves; four launches in a row were all fast (7-12 fps).
    # The title's ray passes that are not left out walk no structure (none is
    # built here): every node test answers "no hit" instead of reading whatever
    # bytes lie there, up to 65,535 steps a wave (fxceless/victory-fork 96ff135):
    # 71-85 ms against 85-117 ms a frame, picture as good (2026-11-26).
    if (-not $env:VILOCITY_BVH_NO_HIT) { $env:VILOCITY_BVH_NO_HIT = "1" }
    # Hair strands run (they were left out: some did not translate and the rest
    # drew streaks); with today's translator fixes Lucia's hair shows, a long
    # ponytail, no streaks (2026-11-26). VILOCITY_HAIR=0: left out again.
    if (-not $env:VILOCITY_HAIR) { $env:VILOCITY_HAIR = "1" }
    # Menus, intro and cutscenes are not held to the title's 30 frames a second
    # (VILOCITY_MAX_FPS=<n>: at most n; unset it for the title's own pace).
    if ($null -eq $env:VILOCITY_MAX_FPS) { $env:VILOCITY_MAX_FPS = "0" }
    if (-not $env:VILOCITY_ZERO_OUT) { $env:VILOCITY_ZERO_OUT = "CS_CombinedReflections,CS_ScreenProbes_Inject,CS_DefragHeap_ComputeRelocations" }
}

# One quoted string: the paths hold spaces ("Vilocity project").
$monitor = Join-Path $PSScriptRoot "monitor-gta6.ps1"
# AMD's PC FidelityFX upscaler (FSR), if downloaded into this folder, runs
# the title's upscaling natively instead of the console build translated.
$ffx = Join-Path $env:LOCALAPPDATA "vilocity\fidelityfx"
# (VILOCITY_FIDELITYFX_DIR=off keeps the console's own upscaler.)
if ($env:VILOCITY_FIDELITYFX_DIR -eq "off") { Remove-Item Env:VILOCITY_FIDELITYFX_DIR }
elseif (Test-Path (Join-Path $ffx "amd_fidelityfx_upscaler_dx12.dll")) { $env:VILOCITY_FIDELITYFX_DIR = $ffx }

$watch = Start-Process powershell -PassThru -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$monitor`" -Log `"$log`""
# The in-game HUD: frame rate and memory on the right of the game's window
# (VILOCITY_HUD=0: none).
if ($env:VILOCITY_HUD -ne "0") {
    $hud = Join-Path $PSScriptRoot "hud-gta6.ps1"
    Start-Process powershell -WindowStyle Hidden -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$hud`" -Log `"$log`"" | Out-Null
}
Write-Host "log: $log"
try {
    $sandbox = @(if ($NoSandbox) { '--no-sandbox' })
    & $exe play $Game @sandbox *> $log
    Write-Host "Vilocity exited with code $LASTEXITCODE (log: $log)"
} finally {
    # The monitor goes with the game (also when this window is closed with Ctrl+C).
    if (-not $watch.HasExited) { Stop-Process -Id $watch.Id -Force -ErrorAction SilentlyContinue }
    Get-ChildItem Env: | Where-Object { $_.Name -like 'VILOCITY_*' -and -not $before.ContainsKey($_.Name) } |
        ForEach-Object { Remove-Item "Env:$($_.Name)" }
    foreach ($name in $before.Keys) { Set-Item "Env:$name" $before[$name] }
}

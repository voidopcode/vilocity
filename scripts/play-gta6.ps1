# Plays GTA VI with Vilocity (in its sandbox) and opens a live monitor window.
#   scripts\play-gta6.ps1 [-Game <folder>] [-Defaults] [-Diag] [-NoSandbox] [-SeriesX] [-NoMonitor]
# -Defaults leaves the frame-rate switches below off (the fork's defaults).
# -Diag runs the diagnostics build (cargo build --profile diag -p vilocity
# --features diagnostics), which logs where each frame's time goes.
# -NoSandbox runs the game as you, outside its sandbox (for measuring).
# -SeriesX shows the title a Series X instead of the default Series S.
# -NoMonitor opens no monitor window (the window launcher shows the state itself).
param(
    [string]$Game = $env:VILOCITY_GTA6_DIR,
    [switch]$Defaults,
    [switch]$Diag,
    [switch]$NoSandbox,
    [switch]$SeriesX,
    [switch]$NoMonitor
)
if (-not $Game) { throw "Pass -Game <game folder> or set VILOCITY_GTA6_DIR" }
# The switches below are set in this window's environment: kept here and put
# back when the game exits, so a later run (with -Defaults, say) starts clean.
$before = @{}
Get-ChildItem Env: | Where-Object Name -like 'VILOCITY_*' | ForEach-Object { $before[$_.Name] = $_.Value }
$root = Split-Path $PSScriptRoot
# The title picks its renderer by console model. As a Series S it renders
# fewer pixels and draws less (some 3,900 draws a frame against 5,000-10,000):
# the mode cool_fox reports a median of 30 fps in.
if (-not $env:VILOCITY_CONSOLE) { $env:VILOCITY_CONSOLE = if ($SeriesX) { "series-x" } else { "series-s" } }
$exe = Join-Path $root $(if ($Diag) { "target\diag\vilocity.exe" } else { "target\release\vilocity.exe" })
$logs = Join-Path $env:LOCALAPPDATA "vilocity\logs"
New-Item -ItemType Directory -Force $logs | Out-Null
$log = Join-Path $logs ("gta6-{0:yyyyMMdd-HHmmss}.log" -f (Get-Date))

if (-not $Defaults) {
    # The native engine is cool_fox's (codec_d's VIctory fork, build 30e54b4):
    # these are the settings of its own launcher, measured there on an RTX 4090
    # (a median of about 30 fps as a Series S). Each can be set before starting.
    $settings = [ordered]@{
        VILOCITY_AMBIENT = "0.8"; VILOCITY_AMBIENT_AIR = "1.5"   # light in the shade, in place of the traced light
        VILOCITY_TEXTURE_NO_WAIT = "1"
        VILOCITY_RELAY = "2"; VILOCITY_RELAY_COPY = "1"; VILOCITY_WIDE_BACK = "1"   # what shaders wrote is copied back by threads of its own
        VILOCITY_CARD_REHASH_RUNS = "1"; VILOCITY_CARD_STEPS = "2"; VILOCITY_CARD_AWAKE = "1"
        VILOCITY_CARD_BUFFERS_MIN_KB = "64"
        VILOCITY_JOIN_COPIES = "1"; VILOCITY_JOIN_PARTS = "8"
        VILOCITY_INDIRECT_WINDOW = "1"; VILOCITY_INDIRECT_BATCH = "2"
        VILOCITY_MAP_THREADS = "1"; VILOCITY_THIN = "1"; VILOCITY_UPLOAD_AHEAD = "1"
        VILOCITY_SHOW_AFTER = "1"
        VILOCITY_PIPELINE_BOOK = "1"   # the pipelines the game makes are kept, to be made ahead (below)
        # No wait between frames: menus, intro and cutscenes are not held to the
        # title's 30 frames a second (16.667: at most 60; 33.333: the title's own).
        VILOCITY_FRAME_INTERVAL_MS = "0"
    }
    # 16 cores or more: more threads translate the lists and copy back (Vice-Tay's
    # counts for a Ryzen 9 9950X3D). Measured here on that processor at 9,400-10,000
    # draws a frame: 55-57 ms against 60-67 ms with the engine's 8, 6 and 4.
    if ([Environment]::ProcessorCount -ge 32) {
        $settings.VILOCITY_THIN_THREADS = "12"; $settings.VILOCITY_UNDER_THREADS = "8"; $settings.VILOCITY_LAND_THREADS = "6"
    }
    # An NVIDIA card keeps its clocks by the driver's power mode: no loop of ours to
    # keep it busy (cool_fox's loop is for AMD cards; Vice-Tay and scarlett-fox leave
    # it off here too). On an RTX 5080 at 3,500-6,500 draws a frame: a median of
    # 35.9 ms against 37.6-39.9 ms with the loop.
    if (@(Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | ForEach-Object PNPDeviceID) -match 'VEN_10DE') {
        $settings.VILOCITY_CARD_AWAKE = "0"
    }
    foreach ($name in $settings.Keys) {
        if (-not (Test-Path "Env:$name")) { Set-Item "Env:$name" $settings[$name] }
    }
}

# One quoted string: the paths hold spaces ("Vilocity project").
$monitor = Join-Path $PSScriptRoot "monitor-gta6.ps1"
# AMD's PC FidelityFX upscaler (FSR), if its DLL is in this folder, runs in
# place of the console build's own (VILOCITY_FSR_DLL=off: the console's).
$fsr = Join-Path $env:LOCALAPPDATA "vilocity\fidelityfx\amd_fidelityfx_upscaler_dx12.dll"
if ($env:VILOCITY_FSR_DLL -eq "off") { Remove-Item Env:VILOCITY_FSR_DLL }
elseif (-not $env:VILOCITY_FSR_DLL -and (Test-Path $fsr)) { $env:VILOCITY_FSR_DLL = $fsr }

# Once per build of the engine: every shader of the game translated ahead (a few
# minutes), so that none is translated while playing.
$cache = if ($env:VILOCITY_CACHE_DIR) { $env:VILOCITY_CACHE_DIR } else { Join-Path $env:LOCALAPPDATA "vilocity\cache" }
$mark = Join-Path $cache "shaders-done.txt"
# (With the settings that change the translation: another set is translated ahead too.)
$build = "{0} {1:o} {2} {3}" -f $exe, (Get-Item $exe).LastWriteTimeUtc, $env:VILOCITY_WAVE_WIDE, $env:VILOCITY_WAVE_WIDE_ONLY
$rpf = Join-Path $Game "shaders.rpf"
if ((Test-Path $rpf) -and ((Get-Content $mark -ErrorAction SilentlyContinue) -ne $build)) {
    Write-Host "Translating the game's shaders ahead (once per build, a few minutes)..."
    & $exe native shaders $rpf
    if ($LASTEXITCODE -eq 0) { New-Item -ItemType Directory -Force $cache | Out-Null; Set-Content $mark $build }
    else { Write-Host "Translating ahead failed: the shaders are translated while playing instead." }
}

# The pipelines earlier runs drew with, made ahead where the game looks for them (the
# driver keeps a sandbox's pipelines apart). Seconds when they are there; minutes after
# an update of Vilocity or of the driver, in place of objects missing while they are
# compiled in play. (VILOCITY_WARM=0: not made ahead.)
if ($env:VILOCITY_WARM -ne "0" -and (Test-Path (Join-Path $cache "pipelines\known-pipelines.bin"))) {
    Write-Host "Checking the shader pipelines made ahead (minutes after an update, seconds otherwise)..."
    if ($NoSandbox) { & $exe native warm --used } else { & $exe warm --used }
}

$watch = if (-not $NoMonitor) { Start-Process powershell -PassThru -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$monitor`" -Log `"$log`"" }
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
    if ($watch -and -not $watch.HasExited) { Stop-Process -Id $watch.Id -Force -ErrorAction SilentlyContinue }
    Get-ChildItem Env: | Where-Object { $_.Name -like 'VILOCITY_*' -and -not $before.ContainsKey($_.Name) } |
        ForEach-Object { Remove-Item "Env:$($_.Name)" }
    foreach ($name in $before.Keys) { Set-Item "Env:$name" $before[$name] }
}

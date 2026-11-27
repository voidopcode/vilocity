# Plays GTA VI with Vilocity (in its sandbox) and opens a live monitor window.
#   scripts\play-gta6.ps1 [-Game <folder>] [-Defaults] [-Diag] [-NoSandbox] [-Picture 1080p|1440p|4k] [-NoMonitor] [-Describe]
# -Describe says what this PC is and which settings it gets, and starts nothing.
# -Defaults leaves the frame-rate switches below off (the fork's defaults).
# -Diag runs the diagnostics build (cargo build --profile diag -p vilocity
# --features diagnostics), which logs where each frame's time goes.
# -NoSandbox runs the game as you, outside its sandbox (for measuring).
# -Picture: 1080p (the default) is the game's Series S picture, 1280x720 upscaled to
# 1920x1080; 1440p and 4k are its Series X picture for a screen of that size (sharper,
# slower: 17-22 fps where 1080p has 24-29 on an RTX 5080). -SeriesX is 4k.
# -NoMonitor opens no monitor window (the window launcher shows the state itself).
param(
    [string]$Game = $env:VILOCITY_GTA6_DIR,
    [switch]$Defaults,
    [switch]$Diag,
    [switch]$NoSandbox,
    [switch]$Describe,
    # A console window that shows the engine's log as it is written.
    [switch]$LogWindow,
    # The game's own ray-traced light (VILOCITY_BVH_BUILDER=1). For AMD Radeon cards, which
    # run its 64-wide programs as they are: other forks measure 10-16 fps with it against 24
    # without on an RX 7900 XTX. On an NVIDIA card it runs at 3-5 fps (measured, RTX 5080).
    [switch]$RayTracing,
    # DLSS on an NVIDIA RTX card, in place of the game's own upscaling: off, or the mode.
    # It runs through NVIDIA's Streamline SDK, which you download yourself (NVIDIA's licence):
    # its bin\x64 folder is named by VILOCITY_STREAMLINE_DIR, or is %LOCALAPPDATA%\vilocity\streamline.
    [ValidateSet('off', 'quality', 'balanced', 'performance', 'dlaa')][string]$Dlss = 'off',
    # DLSS Frame Generation (RTX 40 and 50): a made-up frame between every two, so twice the
    # frame rate shown; the controls answer at the real one. Needs the same Streamline SDK.
    [switch]$FrameGen,
    # Only what a start does before the game (shaders translated, pipelines made ahead), then back.
    [switch]$PrepareOnly,
    # For trying the settings of another PC: "nvidia,8,16,12" is the card's maker, its
    # video memory and the PC's memory in GB, and the processor's threads.
    [string]$As,
    [switch]$SeriesX,
    [ValidateSet('1080p', '1440p', '4k')][string]$Picture = '1080p',
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
if ($SeriesX) { $Picture = '4k' }
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
        # (Not set any more: VILOCITY_JOIN_COPIES=1 and VILOCITY_JOIN_PARTS=8.) With them, and
        # with the game a frame ahead of what is sent, the character was drawn torn (his
        # body pulled into long bands) in single frames while he walks: 183 to 193 of
        # 2,800 frames on the night street, counted frame by frame; without the two and
        # with VILOCITY_SUBMIT_FRAMES=0, 2 to 19, for 1 to 2 ms a frame. None at all only
        # with VILOCITY_CARD_STEPS=0 (58 ms a frame in place of 41): the fault is in there.
        VILOCITY_SUBMIT_FRAMES = "0"
        VILOCITY_INDIRECT_WINDOW = "1"; VILOCITY_INDIRECT_BATCH = "2"
        VILOCITY_MAP_THREADS = "1"; VILOCITY_THIN = "1"; VILOCITY_UPLOAD_AHEAD = "1"
        VILOCITY_SHOW_AFTER = "1"
        VILOCITY_PIPELINE_BOOK = "1"   # the pipelines the game makes are kept, to be made ahead (below)
        # No wait between frames: menus, intro and cutscenes are not held to the
        # title's 30 frames a second (16.667: at most 60; 33.333: the title's own).
        VILOCITY_FRAME_INTERVAL_MS = "0"
    }
    # This PC: the card with the most video memory (its maker and GB), the memory and the
    # processor's threads. (The registry holds a card's memory as 64 bits; WMI stops at 4 GB.)
    $pc = @{ card = "the graphics card"; maker = "other"; vram = 0.0; ram = 0.0; threads = [Environment]::ProcessorCount }
    try {
        $cards = Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}' -ErrorAction SilentlyContinue |
            ForEach-Object { Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue } | Where-Object { $_.'HardwareInformation.qwMemorySize' }
        $best = $cards | Sort-Object { [uint64]$_.'HardwareInformation.qwMemorySize' } -Descending | Select-Object -First 1
        if ($best) {
            $pc.card = $best.DriverDesc; $pc.vram = [uint64]$best.'HardwareInformation.qwMemorySize' / 1GB
            $pc.maker = switch -Regex ($best.MatchingDeviceId) { 'ven_10de' { 'nvidia' } 'ven_1002' { 'amd' } 'ven_8086' { 'intel' } default { 'other' } }
        }
        $pc.ram = (Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue).TotalPhysicalMemory / 1GB
    } catch {}
    if ($As) { $maker, $vram, $ram, $threads = $As -split ','; $pc.maker = $maker; $pc.vram = [double]$vram; $pc.ram = [double]$ram; $pc.threads = [int]$threads; $pc.card = "a $maker card (as asked)" }
    $why = @()
    # The threads that translate the lists, bring memory up to date and land what
    # shaders wrote. 32 threads: measured here on a Ryzen 9 9950X3D at 9,400-10,000 draws
    # a frame, 55-57 ms against 60-67 ms with the engine's 8, 6 and 4. 16 and fewer:
    # Vice-Tay's set for an 8-core processor (more workers than threads lost 41% there).
    # 8 and fewer: half of that, not measured. Between: the engine's own counts.
    if ($pc.threads -ge 32) {
        $settings.VILOCITY_THIN_THREADS = "12"; $settings.VILOCITY_UNDER_THREADS = "8"; $settings.VILOCITY_LAND_THREADS = "6"
        $why += "12, 8 and 6 worker threads"
    } elseif ($pc.threads -le 8) {
        $settings.VILOCITY_THIN_THREADS = "4"; $settings.VILOCITY_UNDER_THREADS = "3"; $settings.VILOCITY_LAND_THREADS = "2"; $settings.VILOCITY_PIPELINE_WORKERS = "2"
        $why += "4, 3 and 2 worker threads (a small processor: expect a low frame rate)"
    } elseif ($pc.threads -le 16) {
        $settings.VILOCITY_THIN_THREADS = "8"; $settings.VILOCITY_UNDER_THREADS = "6"; $settings.VILOCITY_LAND_THREADS = "4"; $settings.VILOCITY_PIPELINE_WORKERS = "3"
        $why += "8, 6 and 4 worker threads"
    }
    # An NVIDIA card keeps its clocks by the driver's power mode: no loop of ours to
    # keep it busy (cool_fox's loop is for AMD cards; Vice-Tay and scarlett-fox leave
    # it off here too). On an RTX 5080 at 3,500-6,500 draws a frame: a median of
    # 35.9 ms against 37.6-39.9 ms with the loop.
    if ($pc.maker -eq 'nvidia') { $settings.VILOCITY_CARD_AWAKE = "0" }
    # (Not set: VILOCITY_WAVE_WIDE_ONLY with TaynanGT/Vice-Tay's list of the game's programs
    # written for waves of 64 lanes. On an RTX 5080 the light-through-fog ones then run and
    # leave grey rectangles on open water, as that project notes too. To try single ones:
    # 6f225b641c5b0708,5dc3b25986622949,369cbe906ea72764,2cc4c197ee29e291,a10179ab7eda0dc3.)
    # Video memory. The game wants about 11 GB of it as a Series S. Under 14 GB a
    # texture's levels get memory when they are used (ZigZagBoi1/ScarlettPort: 18.9 to
    # 22.0 fps on a 16 GB RX 9060 XT); under 10 GB also less of title buffers is kept
    # on the card (Vice-Tay's set for 8 GB cards), and only the Series S picture.
    if ($pc.vram -gt 0 -and $pc.vram -lt 14) { $settings.VILOCITY_LEVELS_ON_DEMAND = "1"; $why += "textures' levels on demand" }
    if ($pc.vram -gt 0 -and $pc.vram -lt 10) {
        $settings.VILOCITY_CARD_BUFFERS_MB = "1536"; $settings.VILOCITY_CARD_WINDOWS_MB = "1024"; $why += "less kept on the card"
        if ($Picture -ne '1080p') { $Picture = '1080p'; $why += "the 1080p picture (the others need more video memory)" }
    }
    if ($RayTracing) {
        $settings.VILOCITY_BVH_BUILDER = "1"
        $why += $(if ($pc.maker -eq 'amd') { "the game's ray tracing" } else { "the game's ray tracing (made for AMD cards: 3-5 fps on an NVIDIA one)" })
    }
    $said = "This PC: {0} with {1:N0} GB, {2:N0} GB of memory, {3} threads. Settings: {4}." -f $pc.card, $pc.vram, $pc.ram, $pc.threads, $(if ($why) { $why -join '; ' } else { "the engine's own" })
    if ($pc.maker -eq 'intel' -or $pc.maker -eq 'other') { $said += " Not tried on this maker's cards." }
    if ($pc.ram -gt 0 -and $pc.ram -lt 30) { $said += " With under 32 GB of memory the game needs a large page file (it takes 23 to 30 GB)." }
    Write-Host $said
    if ($Describe) { foreach ($name in $settings.Keys) { Write-Host ("  {0} = {1}" -f $name, $(if (Test-Path "Env:$name") { (Get-Item "Env:$name").Value + " (set before)" } else { $settings[$name] })) }; return }
    foreach ($name in $settings.Keys) {
        if (-not (Test-Path "Env:$name")) { Set-Item "Env:$name" $settings[$name] }
    }
}
# DLSS: only with an NVIDIA card and the Streamline SDK's files there (louketa/foxhound's way).
if ($Dlss -ne 'off' -and -not $Defaults) {
    $sl = if ($env:VILOCITY_STREAMLINE_DIR) { $env:VILOCITY_STREAMLINE_DIR } else { Join-Path $env:LOCALAPPDATA "vilocity\streamline" }
    if (-not (Test-Path (Join-Path $sl "sl.interposer.dll")) -and (Test-Path (Join-Path $sl "bin\x64\sl.interposer.dll"))) { $sl = Join-Path $sl "bin\x64" }
    if ($pc.maker -eq 'nvidia' -and (Test-Path (Join-Path $sl "sl.interposer.dll"))) {
        $env:VILOCITY_STREAMLINE_DIR = $sl; $env:VILOCITY_DLSS = "1"; $env:VILOCITY_DLSS_MODE = $Dlss
        if ($FrameGen -and (Test-Path (Join-Path $sl "sl.dlss_g.dll"))) { $env:VILOCITY_FRAMEGEN = "1"; Write-Host "DLSS Frame Generation on." }
        Write-Host "DLSS ($Dlss) through the Streamline SDK in $sl."
    } else {
        Write-Host "DLSS is not on: it needs an NVIDIA RTX card and NVIDIA's Streamline SDK (sl.interposer.dll) in $sl."
    }
}
# The console the title is shown, and the size of the screen it is told of.
if (-not $env:VILOCITY_CONSOLE) { $env:VILOCITY_CONSOLE = if ($Picture -eq '1080p') { "series-s" } else { "series-x" } }
if ($Picture -eq '1440p' -and -not $env:VILOCITY_DISPLAY_MODE) { $env:VILOCITY_DISPLAY_MODE = "2560x1440" }

# One quoted string: the paths hold spaces ("Vilocity project").
$monitor = Join-Path $PSScriptRoot "monitor-gta6.ps1"
# AMD's PC FidelityFX upscaler (FSR), if its DLL is in this folder, runs in
# place of the console build's own (VILOCITY_FSR_DLL=off: the console's).
$fsr = Join-Path $env:LOCALAPPDATA "vilocity\fidelityfx\amd_fidelityfx_upscaler_dx12.dll"
if ($env:VILOCITY_FSR_DLL -eq "off") { Remove-Item Env:VILOCITY_FSR_DLL }
elseif (-not $env:VILOCITY_FSR_DLL -and (Test-Path $fsr)) { $env:VILOCITY_FSR_DLL = $fsr }

# One game at a time: two that start together share the card, the caches and DLSS's
# own files (of three started at once, two died while DLSS loaded).
if (Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" | Where-Object CommandLine -like '*native run*') {
    Write-Host "The game is already running: close it first."; return
}

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

# A list of the pipelines the game draws with comes with Vilocity (their names only:
# hashes), so that a PC that has not played yet starts them while the game loads, and
# makes them ahead from its second start on, instead of compiling each when it is first
# seen (2 fps and missing faces in the first minutes). Added to this PC's own list.
$starter = Join-Path (Split-Path $PSScriptRoot) "assets\pipelines-used.txt"
$used = Join-Path $cache "pipelines\early-pipelines.txt"
if (Test-Path $starter) {
    $mine = if (Test-Path $used) { @(Get-Content $used) } else { @() }
    $have = New-Object 'System.Collections.Generic.HashSet[string]' (, [string[]]$mine)
    $more = @(Get-Content $starter | Where-Object { $_ -match '^[0-9a-f]{32}$' -and -not $have.Contains($_) })
    if ($more.Count) {
        New-Item -ItemType Directory -Force (Split-Path $used) | Out-Null
        Add-Content $used $more
        Write-Host "$($more.Count) pipelines other PCs drew with were added to this PC's list."
    }
}

# The pipelines earlier runs drew with, made ahead where the game looks for them (the
# driver keeps a sandbox's pipelines apart). Seconds when they are there; minutes after
# an update of Vilocity or of the driver, in place of objects missing while they are
# compiled in play. (VILOCITY_WARM=0: not made ahead.)
if ($env:VILOCITY_WARM -ne "0" -and (Test-Path (Join-Path $cache "pipelines\known-pipelines.bin"))) {
    Write-Host "Checking the shader pipelines made ahead (minutes after an update, seconds otherwise)..."
    if ($NoSandbox) { & $exe native warm --used } else { & $exe warm --used }
}
if ($PrepareOnly) { Write-Host "Prepared."; return }

$watch = if (-not $NoMonitor) { Start-Process powershell -PassThru -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$monitor`" -Log `"$log`"" }
# The in-game HUD: frame rate and memory on the right of the game's window
# (VILOCITY_HUD=0: none).
if ($env:VILOCITY_HUD -ne "0") {
    $hud = Join-Path $PSScriptRoot "hud-gta6.ps1"
    Start-Process powershell -WindowStyle Hidden -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$hud`" -Log `"$log`"" | Out-Null
}
if ($LogWindow) {
    $view = Join-Path $PSScriptRoot "log-gta6.ps1"
    Start-Process powershell -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$view`" -Log `"$log`"" | Out-Null
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

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
    # (No 'dlaa': with the scene at the size of the output the game stops at its start, on an
    # assertion of its own. Measured at 1080p: quality 1280x720, balanced 1113x626,
    # performance 960x540, every frame by DLSS; 35.4, 35.4 and 34.1 ms a frame.)
    [ValidateSet('off', 'quality', 'balanced', 'performance')][string]$Dlss = 'off',
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

# Windows 10's own Direct3D 12 stops at shader model 6.5, and the game's shaders need 6.6.
# Microsoft's D3D12 Agility SDK runtime has it (Windows 10 22H2 from build 19045.3393). On
# Windows 10 only, it is fetched once from nuget.org, checked against its SHA-512 and
# Microsoft's signature, and only D3D12Core.dll (with Microsoft's licence) is kept in D3D12\
# beside vilocity.exe: each PC gets it from Microsoft, under Microsoft's licence, which the
# repository may not pass on. The engine uses it only where the system's runtime lacks 6.6
# (VILOCITY_AGILITY=0: never).
$agility = Join-Path (Split-Path $exe) "D3D12\D3D12Core.dll"
if ([Environment]::OSVersion.Version.Build -lt 22000 -and -not (Test-Path $agility) -and $env:VILOCITY_AGILITY -ne "0") {
    $ver = "1.620.2"; $sha512 = "1oP/f1cmkGw/Q9KZ9s3UscrSL4MpzNJ7NJLQoUE9mQQQK41UNBm+w7NUXIa/SfalAOv0LZXJmaoJaeXWkwMnNQ=="
    Write-Host "Windows 10: getting Microsoft's D3D12 Agility SDK $ver (35 MB, from nuget.org) for shader model 6.6..."
    $tmp = Join-Path $env:TEMP "vilocity-agility-$ver.nupkg"
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing -Uri "https://api.nuget.org/v3-flatcontainer/microsoft.direct3d.d3d12/$ver/microsoft.direct3d.d3d12.$ver.nupkg" -OutFile $tmp
        $sha = [Security.Cryptography.SHA512]::Create()
        $got = [Convert]::ToBase64String($sha.ComputeHash([IO.File]::ReadAllBytes($tmp)))
        if ($got -ne $sha512) { throw "the package is not the one expected (its SHA-512 differs)" }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        New-Item -ItemType Directory -Force (Split-Path $agility) | Out-Null
        $zip = [IO.Compression.ZipFile]::OpenRead($tmp)
        try {
            [IO.Compression.ZipFileExtensions]::ExtractToFile($zip.GetEntry('build/native/bin/x64/D3D12Core.dll'), $agility, $true)
            [IO.Compression.ZipFileExtensions]::ExtractToFile($zip.GetEntry('LICENSE.txt'), (Join-Path (Split-Path $agility) 'LICENSE.txt'), $true)
        } finally { $zip.Dispose() }
        $signed = Get-AuthenticodeSignature $agility
        if ($signed.Status -ne 'Valid' -or $signed.SignerCertificate.Subject -notlike 'CN=Microsoft Corporation*') {
            Remove-Item $agility -ErrorAction SilentlyContinue
            throw "D3D12Core.dll is not signed by Microsoft"
        }
        Write-Host "Agility SDK runtime in place: $agility (Microsoft's licence beside it)."
    } catch {
        Write-Host "Could not get the Agility SDK: $($_.Exception.Message). Without it Windows 10 has no shader model 6.6 and the game cannot start." -ForegroundColor Yellow
    } finally { Remove-Item $tmp -ErrorAction SilentlyContinue }
}

if (-not $Defaults) {
    # The native engine is cool_fox's (codec_d's VIctory fork, build 30e54b4):
    # these are the settings of its own launcher, measured there on an RTX 4090
    # (a median of about 30 fps as a Series S). Each can be set before starting.
    $settings = [ordered]@{
        # Light in the shade, in place of the traced light: 1.2 (it was 0.8, cool_fox's; against the
        # sun the sides turned to the camera were near black: issue 6).
        VILOCITY_AMBIENT = "1.2"; VILOCITY_AMBIENT_AIR = "1.5"
        VILOCITY_TEXTURE_NO_WAIT = "1"
        VILOCITY_RELAY = "2"; VILOCITY_RELAY_COPY = "1"; VILOCITY_WIDE_BACK = "1"   # what shaders wrote is copied back by threads of its own
        VILOCITY_CARD_REHASH_RUNS = "1"; VILOCITY_CARD_STEPS = "2"; VILOCITY_CARD_AWAKE = "1"
        VILOCITY_CARD_BUFFERS_MIN_KB = "64"
        # (Not set any more: VILOCITY_JOIN_COPIES=1 and VILOCITY_JOIN_PARTS=8.) With them, and
        # with the game a frame ahead of what is sent, the character was drawn torn (his
        # body pulled into long bands) in single frames while he walks: 183 to 222 of
        # 2,800 frames on the night street, counted frame by frame; without the two and
        # with VILOCITY_SUBMIT_FRAMES=0, 2 to 33, for 2 ms a frame. None at all only
        # with VILOCITY_CARD_STEPS=0 (58 ms a frame in place of 41): the fault is in there.
        # The price, measured in the opening: 32 ms a frame at the median in place of 29.
        # (VILOCITY_SUBMIT_DEPTH=1, the launcher's "Steadier picture": at most one submission
        # ahead of what is sent. 2 to 5 torn frames where this leaves 2 to 33, for 1 to 2 ms
        # more; the opening then runs at 33.7 ms at the median against 32.1, under 30 fps.)
        VILOCITY_SUBMIT_FRAMES = "0"
        VILOCITY_INDIRECT_WINDOW = "1"; VILOCITY_INDIRECT_BATCH = "2"
        VILOCITY_MAP_THREADS = "1"; VILOCITY_THIN = "1"; VILOCITY_UPLOAD_AHEAD = "1"
        VILOCITY_SHOW_AFTER = "1"
        VILOCITY_PIPELINE_BOOK = "1"   # the pipelines the game makes are kept, to be made ahead (below)
        # At most 60 frames a second: menus, intro and cutscenes are not held to the
        # title's 30, and not run at 1,000 either (0: no wait; 33.333: the title's own).
        VILOCITY_FRAME_INTERVAL_MS = "16.667"
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
    # 6f225b641c5b0708,5dc3b25986622949,369cbe906ea72764,2cc4c197ee29e291,a10179ab7eda0dc3.
    # Never 6871d6e4b3ac9a10 or 926c46bab3b86bd6: in the wide form they hung an RTX 5080 at
    # the world's start and the whole PC froze and restarted.)
    # Video memory. The game wants about 11 GB of it as a Series S. Under 10 GB less of
    # title buffers is kept on the card (Vice-Tay's set for 8 GB cards), and only the
    # Series S picture. (Textures' levels on demand, VILOCITY_LEVELS_ON_DEMAND=1, are no
    # longer set under 14 GB: on this 16 GB card held to about 8 GB, the same day walk ran
    # at 13 fps median with black objects with them, 32 fps without, 2026-11-27.)
    if ($pc.vram -gt 0 -and $pc.vram -lt 10) {
        $settings.VILOCITY_CARD_BUFFERS_MB = "1536"; $settings.VILOCITY_CARD_WINDOWS_MB = "1024"; $why += "less kept on the card"
    }
    # The Series X pictures (1440p, 4K) take about 13 GB of the card (an RTX 5080, 2026-11-27):
    # on a 12 GB card they would not fit, and a full card gave 4-7 fps.
    if ($pc.vram -gt 0 -and $pc.vram -lt 14 -and $Picture -ne '1080p') { $Picture = '1080p'; $why += "the 1080p picture (1440p and 4K take about 13 GB of the card)" }
    # And under 24 GB of RAM: the Series X pictures take about 27 GB, the 1080p one about 22 (2026-11-27).
    if ($pc.ram -gt 0 -and $pc.ram -lt 24 -and $Picture -ne '1080p') { $Picture = '1080p'; $why += "the 1080p picture (1440p and 4K take about 27 GB of memory)" }
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
# The in-game menu's settings that apply at a start (F1, marked *): the upscaler and frame
# generation, kept in menu.txt in the data folder, in place of -Dlss and -FrameGen.
$dataDir = if ($env:VILOCITY_DATA_DIR) { $env:VILOCITY_DATA_DIR } else { Join-Path $env:LOCALAPPDATA "vilocity\data" }
$menu = @{}
Get-Content (Join-Path $dataDir "menu.txt") -ErrorAction SilentlyContinue | ForEach-Object { if ($_ -match '^\s*(\w+)\s*=\s*([\d.]+)\s*$') { $menu[$Matches[1]] = [double]$Matches[2] } }
if (-not $Defaults -and $menu.ContainsKey('upscaler')) {
    $Dlss = @('off', 'quality', 'balanced', 'performance')[[math]::Min(3, [int]$menu.upscaler)]
    Write-Host ("Upscaler from the in-game menu (F1): " + $(if ($Dlss -eq 'off') { "the game's own (AMD FSR)" } else { "DLSS $Dlss" }) + ".")
}
if (-not $Defaults -and $menu.ContainsKey('framegen')) { $FrameGen = $menu.framegen -ge 2 }
if (-not $Defaults -and $menu.ContainsKey('picture')) { $Picture = @('1080p', '1440p', '4k')[[math]::Min(2, [int]$menu.picture)] }
if (-not $Defaults -and $menu.dlssmodel -ge 1) { $env:VILOCITY_DLSS_PRESET = @('', 'j', 'k', 'l', 'm')[[math]::Min(4, [int]$menu.dlssmodel)] }
# (The title's heat haze, CS_IndirectApplyDiffusionInScattering_HeatHaze, left out.)
if (-not $Defaults -and $menu.haze -ge 1) { $env:VILOCITY_LEAVE_OUT = (@($env:VILOCITY_LEAVE_OUT, '731a39171b568298') | Where-Object { $_ }) -join ',' }
# The title's motion blur left out unless F1 or the launcher turned it on (the user's choice, 2026-11-28).
if (-not $Defaults -and -not ($menu.blur -ge 1)) { $env:VILOCITY_LEAVE_OUT = (@($env:VILOCITY_LEAVE_OUT, '51e19585b4753814') | Where-Object { $_ }) -join ',' }
# The torn-frame fix: the steps done on the card ahead of the flights left out (no torn frames, much slower).
if (-not $Defaults -and $menu.tornfix -ge 1) { $env:VILOCITY_CARD_STEPS = "0" }
# DLSS: only with an NVIDIA card and the Streamline SDK's files there (louketa/foxhound's way).
if ($Dlss -ne 'off' -and -not $Defaults) {
    $sl = if ($env:VILOCITY_STREAMLINE_DIR) { $env:VILOCITY_STREAMLINE_DIR } else { Join-Path $env:LOCALAPPDATA "vilocity\streamline" }
    if (-not (Test-Path (Join-Path $sl "sl.interposer.dll")) -and (Test-Path (Join-Path $sl "bin\x64\sl.interposer.dll"))) { $sl = Join-Path $sl "bin\x64" }
    if ($pc.maker -eq 'nvidia' -and (Test-Path (Join-Path $sl "sl.interposer.dll"))) {
        $env:VILOCITY_STREAMLINE_DIR = $sl; $env:VILOCITY_DLSS = "1"; $env:VILOCITY_DLSS_MODE = $Dlss
        if ($FrameGen -and (Test-Path (Join-Path $sl "sl.dlss_g.dll"))) {
            # Streamline refuses frame generation while Windows' GPU scheduling is off.
            # Windows' GPU scheduling: HwSchMode 2 is on, 1 off; on Windows 11 the value is often
            # not there while it is on, its default (issue 2: an RTX 5070 Ti, Windows 11 25H2).
            $hags = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' -ErrorAction SilentlyContinue).HwSchMode
            if ($hags -eq 2 -or ($null -eq $hags -and [Environment]::OSVersion.Version.Build -ge 22000)) { $env:VILOCITY_FRAMEGEN = "1"; Write-Host "DLSS Frame Generation on." }
            else { Write-Host "Frame generation needs Windows' hardware-accelerated GPU scheduling (Settings, System, Display, Graphics; then a restart): it is off, so DLSS runs alone." }
        }
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
    # (Its lines also go to logs\shaders-progress.txt as they come, one each 1,000 shaders: the
    # launcher shows how far it is; two players thought the step stuck, 2026-11-27.)
    $shading = Join-Path $env:LOCALAPPDATA "vilocity\logs\shaders-progress.txt"
    New-Item -ItemType Directory -Force (Split-Path $shading) | Out-Null
    & $exe native shaders $rpf | Tee-Object -FilePath $shading | Out-Host
    if ($LASTEXITCODE -eq 0) {
        New-Item -ItemType Directory -Force $cache | Out-Null; Set-Content $mark $build
        # How many there were, for the launcher's bar the next time.
        $all = Select-String -Path $shading -Pattern '^(\d+) shaders in the cache, \d+ do not translate' | Select-Object -Last 1
        if ($all) { Set-Content (Join-Path $cache "shaders-count.txt") $all.Matches[0].Groups[1].Value }
    }
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
    # (Its lines also go to logs\warm-progress.txt as they come: the launcher shows how far it is.)
    $progress = Join-Path $env:LOCALAPPDATA "vilocity\logs\warm-progress.txt"
    New-Item -ItemType Directory -Force (Split-Path $progress) | Out-Null
    if ($NoSandbox) { & $exe native warm --used | Tee-Object -FilePath $progress | Out-Host } else { & $exe warm --used | Tee-Object -FilePath $progress | Out-Host }
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
    $code = $LASTEXITCODE
    Write-Host "Vilocity exited with code $code (log: $log)"
    # (Issue 3: a PC where Windows refused the sandbox its own folders.)
    if (-not $NoSandbox -and $code -ne 0 -and (Select-String -Path $log -Pattern '(os error 5)' -SimpleMatch -Quiet)) {
        Write-Host "Windows refused the sandbox a folder (access denied). To play without the sandbox: launcher, Options, untick Sandbox. Please attach this log to an issue."
    }
    # (Issue 3: AMD's compiler crashed reading the pipeline library an earlier run saved, two seconds in;
    # deleting the cache cured it. Only the library goes: the next start makes it again.)
    # (The log's lines may be wrapped at the console's width: looked for in it joined.)
    if ($code -ne 0 -and ((Get-Content $log -Raw -ErrorAction SilentlyContinue) -replace "`r?`n", '') -match 'exception 0x[0-9a-f]+ at (amdxc64|nvwgf2umx|igd12umd64)\.dll') {
        Remove-Item (Join-Path $cache "pipelines\host-pipelines.bin"), (Join-Path $cache "pipelines\host-pipelines.for") -ErrorAction SilentlyContinue
        Write-Host "The graphics driver crashed reading the pipelines saved by an earlier run: they are removed, and the next start makes them again (slower, once)."
    }
} finally {
    # The monitor goes with the game (also when this window is closed with Ctrl+C).
    if ($watch -and -not $watch.HasExited) { Stop-Process -Id $watch.Id -Force -ErrorAction SilentlyContinue }
    Get-ChildItem Env: | Where-Object { $_.Name -like 'VILOCITY_*' -and -not $before.ContainsKey($_.Name) } |
        ForEach-Object { Remove-Item "Env:$($_.Name)" }
    foreach ($name in $before.Keys) { Set-Item "Env:$name" $before[$name] }
}

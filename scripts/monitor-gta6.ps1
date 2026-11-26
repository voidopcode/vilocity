# Live view of a Vilocity GTA VI run: frame rate, memory, processor and GPU
# use, where each frame's time goes, and the warnings and errors of the log.
#   scripts\monitor-gta6.ps1 [-Log <file>]   (default: the newest log)
param([string]$Log)
$logs = Join-Path $env:LOCALAPPDATA "vilocity\logs"
if (-not $Log) {
    $Log = Get-ChildItem $logs -Filter *.log -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime | Select-Object -Last 1 -ExpandProperty FullName
}
$host.UI.RawUI.WindowTitle = "Vilocity monitor"
$cores = [Environment]::ProcessorCount
$pos = 0L
$frame = ""; $card = ""; $tex = ""
$issues = [ordered]@{}   # message -> [count, last time, level]
$lastCpu = $null; $lastAt = $null
$started = Get-Date

function Read-New {
    if (-not $Log -or -not (Test-Path $Log)) { return @() }
    $fs = [IO.File]::Open($Log, 'Open', 'Read', 'ReadWrite')
    try {
        if ($fs.Length -lt $script:pos) { $script:pos = 0 }
        $fs.Seek($script:pos, 'Begin') | Out-Null
        $text = (New-Object IO.StreamReader($fs)).ReadToEnd()
        $script:pos = $fs.Length
    } finally { $fs.Close() }
    $text -split "`r?`n"
}

while ($true) {
    foreach ($line in Read-New) {
        if ($line -match 'vilocity_gpu::host: [\d.]+ fps, ') { $frame = $line -replace '^.*vilocity_gpu::host: ', '' }
        elseif ($line -match "the card's clock per frame") { $card = $line -replace '^.*per frame: ', '' }
        elseif ($line -match 'texture refreshes per frame') { $tex = $line -replace '^.*all threads: ', '' }
        elseif ($line -match ' (WARN|ERROR) (.*)$' -or $line -match '^(Vilocity): (.*)$') {
            $level = $Matches[1]
            # Group by the message without its numbers.
            $key = ($Matches[2] -replace '0x[0-9a-f]+|\d+(\.\d+)?', '#')
            if ($key.Length -gt 150) { $key = $key.Substring(0, 150) }
            $time = if ($line -match 'T(\d\d:\d\d:\d\d)') { $Matches[1] } else { (Get-Date -Format HH:mm:ss) }
            if ($issues.Contains($key)) { $issues[$key][0]++; $issues[$key][1] = $time }
            else { $issues[$key] = @(1, $time, $level) }
        }
    }

    $game = Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" |
        Where-Object CommandLine -like '*native run*' | Select-Object -First 1
    $g = if ($game) { Get-Process -Id $game.ProcessId -ErrorAction SilentlyContinue }
    # The window closes with the game (once it has started).
    if ($g) { $seen = $true } elseif ($seen) { exit }
    $gpu = (nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw --format=csv,noheader,nounits 2>$null) -split ',\s*'
    $os = Get-CimInstance Win32_OperatingSystem

    # The HUD block: frame rate and what the game and the PC use.
    $mb = 1MB
    $gpuName = if (-not $script:gpuName) { $script:gpuName = (nvidia-smi --query-gpu=name --format=csv,noheader 2>$null) -replace 'NVIDIA GeForce ', ''; $script:gpuName } else { $script:gpuName }
    $hud = @()
    $fps = if ($frame -match '^([\d.]+) fps, ([\d.]+) ms/frame') { "{0} fps  ({1} ms a frame)" -f $Matches[1], $Matches[2] } else { "-" }
    $pcCpu = (Get-CimInstance Win32_Processor | Measure-Object LoadPercentage -Average).Average
    $gameCpu = "-"; $vGame = "-"; $vShared = "-"; $ramGame = "-"; $commitGame = "-"
    if ($g) {
        if ($lastCpu) { $gameCpu = "{0:N1}%" -f (($g.TotalProcessorTime - $lastCpu).TotalSeconds / ((Get-Date) - $lastAt).TotalSeconds / $cores * 100) }
        $ramGame = "{0:N0} MB" -f ($g.WorkingSet64 / $mb)
        $commitGame = "{0:N0} MB" -f ($g.PrivateMemorySize64 / $mb)
        $pidTag = "pid_$($g.Id)_*"
        $vm = Get-CimInstance Win32_PerfRawData_GPUPerformanceCounters_GPUProcessMemory -ErrorAction SilentlyContinue | Where-Object Name -like $pidTag
        if ($vm) {
            $vGame = "{0:N0} MB" -f (($vm | Measure-Object DedicatedUsage -Sum).Sum / $mb)
            $vShared = "{0:N0} MB" -f (($vm | Measure-Object SharedUsage -Sum).Sum / $mb)
        }
    }
    $hud += , @("VILOCITY HUD ($gpuName)  {0:N0}s" -f ((Get-Date) - $started).TotalSeconds, 'Cyan')
    $hud += , @("FPS:        $fps", 'Green')
    $hud += , @("CPU:        Game $gameCpu | PC $pcCpu%", 'Cyan')
    if ($gpu.Count -ge 5) { $hud += , @(("VRAM GPU:   {0}/{1} MB ({2:N1}%)" -f $gpu[1], $gpu[2], ($gpu[1] / $gpu[2] * 100)), 'Yellow') }
    $hud += , @("VRAM Game:  $vGame | Shrd: $vShared", 'Yellow')
    $hud += , @("RAM Game:   $ramGame", 'Cyan')
    $hud += , @("Commit Game: $commitGame", 'Yellow')
    $used = ($os.TotalVisibleMemorySize - $os.FreePhysicalMemory) / 1KB; $total = $os.TotalVisibleMemorySize / 1KB
    $hud += , @(("PC RAM:     {0:N0}/{1:N0} MB ({2:N1}%)" -f $used, $total, ($used / $total * 100)), $(if ($used / $total -gt 0.9) { 'Red' } else { 'Cyan' }))
    $cUsed = ($os.TotalVirtualMemorySize - $os.FreeVirtualMemory) / 1KB; $cTotal = $os.TotalVirtualMemorySize / 1KB
    $hud += , @(("PC Commit:  {0:N0}/{1:N0} MB ({2:N1}%)" -f $cUsed, $cTotal, ($cUsed / $cTotal * 100)), 'Cyan')

    $out = New-Object System.Text.StringBuilder
    $add = { param($s) [void]$out.AppendLine($s) }
    & $add ("VILOCITY MONITOR   {0:HH:mm:ss}   up {1:hh\:mm\:ss}   log {2}" -f (Get-Date), ((Get-Date) - $started), (Split-Path $Log -Leaf))
    & $add ("=" * 100)
    if ($g) {
        $cpu = ""
        if ($lastCpu) {
            $pct = ($g.TotalProcessorTime - $lastCpu).TotalSeconds / ((Get-Date) - $lastAt).TotalSeconds / $cores * 100
            $cpu = "{0:N0}% of all cores" -f $pct
        }
        $lastCpu = $g.TotalProcessorTime; $lastAt = Get-Date
        $title = $g.MainWindowTitle -replace '^.*\| Vilocity ', ''
        & $add ("GAME     running (sandboxed)   {0}" -f $title)
        & $add ("         memory {0:N1} GB   processor {1}   threads {2}   responding {3}" -f ($g.WorkingSet64 / 1GB), $cpu, $g.Threads.Count, $g.Responding)
    } else {
        & $add "GAME     not running"
        $lastCpu = $null
    }
    if ($gpu.Count -ge 5) {
        & $add ("GPU      busy {0}%   video memory {1:N1} / {2:N1} GB   {3} C   {4} W" -f $gpu[0], ($gpu[1] / 1024), ($gpu[2] / 1024), $gpu[3], $gpu[4])
    }
    & $add ("PC       memory free {0:N1} of {1:N1} GB" -f ($os.FreePhysicalMemory / 1MB), ($os.TotalVisibleMemorySize / 1MB))
    & $add ""
    & $add "WHERE THE FRAME'S TIME GOES"
    & $add ("  " + $(if ($frame) { $frame } else { "(no frame line yet)" }))
    if ($card) { & $add ("  card: " + $card) }
    if ($tex) { & $add ("  textures: " + $tex) }
    & $add ""
    & $add ("WARNINGS AND ERRORS ({0} kinds; newest last)" -f $issues.Count)
    $issues.GetEnumerator() | Sort-Object { $_.Value[1] } | Select-Object -Last 14 | ForEach-Object {
        $v = $_.Value
        & $add ("  {0} {1,-5} x{2,-6} {3}" -f $v[1], $v[2], $v[0], $_.Key)
    }
    Clear-Host
    foreach ($h in $hud) { Write-Host $h[0] -ForegroundColor $h[1] }
    Write-Host ""
    Write-Host $out.ToString()
    Start-Sleep -Seconds 2
}

# In-game HUD for a Vilocity GTA VI run: a small see-through panel on the right
# of the game's window with the frame rate and what the game and the PC use.
# It stays on top, lets clicks through to the game, follows the window, and
# closes with the game.   scripts\hud-gta6.ps1 [-Log <file>]
param([string]$Log)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public class HudWin {
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern int SetWindowLong(IntPtr h, int i, int v);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  public struct RECT { public int L, T, R, B; }
}
'@
$logs = Join-Path $env:LOCALAPPDATA "vilocity\logs"
$cores = [Environment]::ProcessorCount
$state = @{ pos = 0L; frame = ""; lastCpu = $null; lastAt = $null; started = Get-Date; seen = $false; log = $Log }

function Read-Frame {
    if (-not $state.log) {
        $state.log = Get-ChildItem $logs -Filter *.log -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime | Select-Object -Last 1 -ExpandProperty FullName
    }
    if (-not $state.log -or -not (Test-Path $state.log)) { return }
    $fs = [IO.File]::Open($state.log, 'Open', 'Read', 'ReadWrite')
    try {
        if ($fs.Length -lt $state.pos) { $state.pos = 0 }
        # Only the tail matters: the newest frame line.
        if ($fs.Length - $state.pos -gt 4MB) { $state.pos = $fs.Length - 4MB }
        $fs.Seek($state.pos, 'Begin') | Out-Null
        $text = (New-Object IO.StreamReader($fs)).ReadToEnd()
        $state.pos = $fs.Length
    } finally { $fs.Close() }
    foreach ($line in $text -split "`r?`n") {
        if ($line -match 'vilocity_gpu::host: ([\d.]+) fps, ([\d.]+) ms/frame') { $state.frame = "{0} fps  ({1} ms)" -f $Matches[1], $Matches[2] }
    }
}

$form = New-Object Windows.Forms.Form
$form.FormBorderStyle = 'None'; $form.TopMost = $true; $form.ShowInTaskbar = $false
$form.BackColor = [Drawing.Color]::FromArgb(12, 14, 20); $form.Opacity = 0.7
$form.StartPosition = 'Manual'; $form.AutoSize = $true; $form.AutoSizeMode = 'GrowAndShrink'
$form.Padding = New-Object Windows.Forms.Padding(4, 3, 4, 3)
$label = New-Object Windows.Forms.Label
$label.Font = New-Object Drawing.Font("Consolas", 8.5)
$label.AutoSize = $true; $label.ForeColor = [Drawing.Color]::FromArgb(150, 230, 150)
$label.Location = New-Object Drawing.Point(4, 3); $form.Controls.Add($label)
# Click-through, and not activated by clicks: the game keeps focus and input.
$form.Add_Shown({
    $ex = [HudWin]::GetWindowLong($form.Handle, -20)
    [HudWin]::SetWindowLong($form.Handle, -20, $ex -bor 0x80000 -bor 0x20 -bor 0x08000000) | Out-Null
})
# One reading that fails shows "-", not an empty panel.
function Try-Get([scriptblock]$f) { try { & $f } catch { "-" } }

$timer = New-Object Windows.Forms.Timer
$timer.Interval = 1000
$timer.Add_Tick({
    $game = Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" |
        Where-Object CommandLine -like '*native run*' | Select-Object -First 1
    $g = if ($game) { Get-Process -Id $game.ProcessId -ErrorAction SilentlyContinue }
    if ($g) { $state.seen = $true } elseif ($state.seen) { $form.Close(); return }
    Read-Frame
    $mb = 1MB
    $text = @()
    $text += "FPS   " + $(if ($state.frame) { $state.frame } else { "-" })
    $gameCpu = "-"
    if ($g) {
        if ($state.lastCpu) {
            $gameCpu = Try-Get { "{0:N0}%" -f (($g.TotalProcessorTime - $state.lastCpu).TotalSeconds / ((Get-Date) - $state.lastAt).TotalSeconds / $cores * 100) }
        }
        $state.lastCpu = $g.TotalProcessorTime; $state.lastAt = Get-Date
    }
    $pcCpu = Try-Get { "{0:N0}%" -f (Get-CimInstance Win32_Processor | Measure-Object LoadPercentage -Average).Average }
    $text += "CPU   game $gameCpu  pc $pcCpu"
    $text += Try-Get {
        $v = (nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader,nounits) -split ',\s*'
        "VRAM  {0:N1}/{1:N1} GB" -f ([double]$v[0] / 1024), ([double]$v[1] / 1024)
    }
    if ($g) {
        $text += Try-Get {
            $vm = Get-CimInstance Win32_PerfRawData_GPUPerformanceCounters_GPUProcessMemory | Where-Object Name -like "pid_$($g.Id)_*"
            "game  vram {0:N1} GB  shared {1:N1} GB" -f (($vm | Measure-Object DedicatedUsage -Sum).Sum / 1GB), (($vm | Measure-Object SharedUsage -Sum).Sum / 1GB)
        }
        $text += Try-Get { "      ram {0:N1} GB  commit {1:N1} GB" -f ($g.WorkingSet64 / 1GB), ($g.PrivateMemorySize64 / 1GB) }
        $r = New-Object HudWin+RECT
        if (-not [HudWin]::IsIconic($g.MainWindowHandle) -and [HudWin]::GetWindowRect($g.MainWindowHandle, [ref]$r)) {
            $form.Location = New-Object Drawing.Point(($r.R - $form.Width - 10), ($r.T + 36))
        }
    }
    $text += Try-Get {
        $os = Get-CimInstance Win32_OperatingSystem
        $used = ($os.TotalVisibleMemorySize - $os.FreePhysicalMemory) / 1MB; $total = $os.TotalVisibleMemorySize / 1MB
        "PC    ram {0:N1}/{1:N0} GB" -f $used, $total
    }
    $label.Text = $text -join "`n"
})
$timer.Start()
[Windows.Forms.Application]::Run($form)

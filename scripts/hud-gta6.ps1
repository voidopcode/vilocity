# In-game panel for a Vilocity GTA VI run: the frame rate, and what the game and
# the PC use, in the upper right corner of the game's window.
# It goes with the game's window: shown whenever the game's picture is seen at that
# corner (whichever window has the keyboard), gone when another window covers it or
# the game is minimised, and closed with the game. Clicks go through it to the
# game, and it never takes the keyboard. (Not the game window's owned window:
# Windows joins the input queues of an owner and what it owns, and the game then
# got no key and no mouse while this script was busy.)
# F8 (while the game is in front) hides and shows it.   scripts\hud-gta6.ps1 [-Log <file>]
param([string]$Log)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -ReferencedAssemblies System.Windows.Forms, System.Drawing @'
using System; using System.Runtime.InteropServices; using System.Windows.Forms;
// A window that is never made the active one, and lets clicks through
// (layered, transparent to the mouse, not activated, no button in Alt+Tab).
public class HudForm : Form {
  public HudForm() { DoubleBuffered = true; }
  protected override bool ShowWithoutActivation { get { return true; } }
  protected override CreateParams CreateParams { get { CreateParams p = base.CreateParams; p.ExStyle |= 0x80000 | 0x20 | 0x08000000 | 0x80 | 0x8; return p; } }
}
public class HudWin {
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int height, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint process);
  // Whether the window the user sees at this point of the screen is one of the game's
  // process; the panel itself lets the mouse through, so it is not what is found there.
  public static bool Seen(IntPtr game, int x, int y) {
    POINT p = new POINT(); p.X = x; p.Y = y; IntPtr w = WindowFromPoint(p);
    uint its, games; GetWindowThreadProcessId(w, out its); GetWindowThreadProcessId(game, out games);
    return w != IntPtr.Zero && its == games;
  }
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int vk);
  [DllImport("kernel32.dll")] public static extern bool GlobalMemoryStatusEx(ref MEMORY m);
  public struct RECT { public int L, T, R, B; }
  public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct MEMORY { public uint Size, Load; public ulong Total, Free, PageTotal, PageFree, VirtualTotal, VirtualFree, Extended; }
  // Used and all of the PC's memory, in GB.
  public static double[] Ram() { MEMORY m = new MEMORY(); m.Size = 64; GlobalMemoryStatusEx(ref m); return new double[] { (m.Total - m.Free) / 1073741824.0, m.Total / 1073741824.0 }; }
}
'@
$cores = [Environment]::ProcessorCount
# (Names differ by more than their case: PowerShell takes $H and $h for one variable,
# and the panel's height was the game's window handle inside a function that took one.)
# What the panel shows; $null: not known (yet).
$s = @{ fps = $null; ms = $null; draws = $null; gameCpu = $null; pcCpu = $null; gpu = $null; vramUsed = $null; vramAll = $null
        gameVram = $null; gameShared = $null; gameRam = $null; gameCommit = $null; ram = $null; ramAll = $null }
$state = @{ keys = "on foot"; logAt = 0L; id = 0; owner = [IntPtr]::Zero; lastCpu = $null; lastAt = $null; seen = $false; hidden = $false; f8 = $false; tick = 0 }
$pc = try { New-Object Diagnostics.PerformanceCounter('Processor', '% Processor Time', '_Total') } catch { $null }
# The card's memory in GB, for cards without nvidia-smi (the largest the driver reports).
$cardAll = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*' -ErrorAction SilentlyContinue |
    ForEach-Object { [double]$_.'HardwareInformation.qwMemorySize' } | Measure-Object -Maximum).Maximum / 1GB

$PW = 250; $PH = 152
# The panel never takes the processor from the game.
try { [Diagnostics.Process]::GetCurrentProcess().PriorityClass = 'BelowNormal' } catch {}
$form = New-Object HudForm
$form.FormBorderStyle = 'None'; $form.TopMost = $true; $form.ShowInTaskbar = $false
$form.BackColor = [Drawing.Color]::FromArgb(14, 16, 22); $form.Opacity = 0.86
$form.StartPosition = 'Manual'; $form.Size = New-Object Drawing.Size($PW, $PH); $form.Location = New-Object Drawing.Point(-4000, -4000)
# Rounded corners.
$path = New-Object Drawing.Drawing2D.GraphicsPath; $rad = 14
$path.AddArc(0, 0, $rad, $rad, 180, 90); $path.AddArc($PW - $rad, 0, $rad, $rad, 270, 90)
$path.AddArc($PW - $rad, $PH - $rad, $rad, $rad, 0, 90); $path.AddArc(0, $PH - $rad, $rad, $rad, 90, 90); $path.CloseFigure()
$form.Region = New-Object Drawing.Region($path)

$big = New-Object Drawing.Font("Segoe UI", 21, [Drawing.FontStyle]::Bold)
$unit = New-Object Drawing.Font("Segoe UI", 9)
$name = New-Object Drawing.Font("Segoe UI", 7.5, [Drawing.FontStyle]::Bold)
$value = New-Object Drawing.Font("Segoe UI", 8.5)
$white = [Drawing.Color]::FromArgb(236, 238, 244); $grey = [Drawing.Color]::FromArgb(138, 144, 160)
$good = [Drawing.Color]::FromArgb(61, 220, 132); $fair = [Drawing.Color]::FromArgb(250, 204, 21); $bad = [Drawing.Color]::FromArgb(248, 113, 113)
$brand = [Drawing.Color]::FromArgb(124, 92, 255); $track = [Drawing.Color]::FromArgb(40, 44, 56)
function N($v, $f = "N1") { if ($null -eq $v) { "-" } else { ([double]$v).ToString($f) } }

$form.Add_Paint({
    param($sender, $e)
    $g = $e.Graphics; $g.SmoothingMode = 'AntiAlias'; $g.TextRenderingHint = 'ClearTypeGridFit'
    $brush = { param($c) New-Object Drawing.SolidBrush $c }
    # The frame rate, in the colour of how it plays; the frame's time and draws beside it.
    $colour = if ($null -eq $s.fps) { $grey } elseif ($s.fps -ge 28) { $good } elseif ($s.fps -ge 20) { $fair } else { $bad }
    $g.FillRectangle((& $brush $colour), 0, 0, 4, $PH)
    $text = N $s.fps "N0"
    $g.DrawString($text, $big, (& $brush $colour), 12, 4)
    $wide = $g.MeasureString($text, $big).Width
    $g.DrawString("fps", $unit, (& $brush $grey), 10 + $wide, 20)
    $right = New-Object Drawing.StringFormat; $right.Alignment = 'Far'
    $g.DrawString((N $s.ms) + " ms", $value, (& $brush $white), ($PW - 12), 10, $right)
    if ($null -ne $s.draws) { $g.DrawString(("{0:N0} draws" -f $s.draws), $name, (& $brush $grey), ($PW - 12), 26, $right) }
    # A line: its name, what it says, and a bar of how full it is ($null: no bar).
    $row = { param($y, $what, $says, $part, $whole, $inner, $tint = $white)
        $g.DrawString($what, $name, (& $brush $grey), 13, $y + 1)
        $g.DrawString($says, $value, (& $brush $tint), 50, $y)
        if ($null -ne $part -and $whole) {
            $g.FillRectangle((& $brush $track), 50, $y + 16, ($PW - 62), 3)
            $share = [math]::Min(1.0, [double]$part / [double]$whole)
            $g.FillRectangle((& $brush $(if ($share -gt 0.92) { $bad } else { $grey })), 50, $y + 16, [int](($PW - 62) * $share), 3)
            if ($null -ne $inner) { $g.FillRectangle((& $brush $brand), 50, $y + 16, [int](($PW - 62) * [math]::Min(1.0, [double]$inner / [double]$whole)), 3) }
        }
    }
    & $row 46 "CPU" ("game {0}%   PC {1}%" -f (N $s.gameCpu "N0"), (N $s.pcCpu "N0")) $null $null $null
    & $row 64 "GPU" ("{0}%   {1} of {2} GB" -f (N $s.gpu "N0"), (N $s.vramUsed), (N $s.vramAll)) $s.vramUsed $s.vramAll $s.gameVram
    # What other programs hold of the card: from 2 GB on it is said, in place of the shared
    # memory (with 6 GB held by others the game's textures waited: blank eyes, wrong skin).
    $others = if ($null -ne $s.vramUsed -and $null -ne $s.gameVram) { $s.vramUsed - $s.gameVram } else { 0 }
    # A full card: Windows moves the game's memory to the PC's and the card's frame takes 60-120 ms
    # (4-7 fps at 1440p with 5.6 GB held by others, 2026-11-27; 1080p needs some 5 GB less).
    $full = $null -ne $s.vramUsed -and $s.vramAll -gt 0 -and ($s.vramUsed / $s.vramAll) -gt 0.93
    if ($full) {
        # (Short enough for the panel: "CARD FULL: others 7.3 GB (vramhold 3.0)" ran past its edge.)
        $text = if ($s.topName -and $s.topVram -ge 1) { "CARD FULL: {0} {1} GB" -f $s.topName.Substring(0, [math]::Min(10, $s.topName.Length)), (N $s.topVram) } else { "CARD FULL: others {0} GB" -f (N $others) }
        & $row 86 "GAME" $text $null $null $null $bad
    }
    else { & $row 86 "GAME" $(if ($others -ge 2) { "video {0} GB   others hold {1} GB" -f (N $s.gameVram), (N $others) } else { "video {0} GB   shared {1} GB" -f (N $s.gameVram), (N $s.gameShared) }) $null $null $null }
    & $row 104 "RAM" ("game {0}   PC {1} of {2} GB" -f (N $s.gameRam), (N $s.ram), (N $s.ramAll "N0")) $s.ram $s.ramAll $s.gameRam
    & $row 126 "KEYS" $(if ($state.keys -eq "driving") { "driving   G: on foot   F1: settings" } else { "on foot   G: driving   F1: settings" }) $null $null $null
})

# The game's process and window: looked for until found, then kept.
function Game {
    if ($state.id) { $p = Get-Process -Id $state.id -ErrorAction SilentlyContinue; if ($p) { return $p }; $state.id = 0 }
    $game = Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" | Where-Object CommandLine -like '*native run*' | Select-Object -First 1
    if ($game) { $state.id = $game.ProcessId; Get-Process -Id $game.ProcessId -ErrorAction SilentlyContinue }
}
# In the upper right corner of the game's picture; shown with the game's window.
function Place($win) {
    $r = New-Object HudWin+RECT; $p = New-Object HudWin+POINT
    $shown = [HudWin]::IsWindowVisible($win) -and -not [HudWin]::IsIconic($win) -and [HudWin]::GetClientRect($win, [ref]$r) -and [HudWin]::ClientToScreen($win, [ref]$p)
    if ($shown) {
        $at = New-Object Drawing.Point(($p.X + $r.R - $PW - 14), ($p.Y + 14)); if ($form.Location -ne $at) { $form.Location = $at }
        # Only where the game's picture is what the user sees there (the panel is over all
        # windows, as it must be to be over the game's without belonging to it): asked
        # just beside and just under the panel, where the panel itself is not found.
        $shown = [HudWin]::Seen($win, ($at.X - 5), ($at.Y + $PH / 2)) -and [HudWin]::Seen($win, ($at.X + $PW / 2), ($at.Y + $PH + 5))
    }
    $want = $shown -and -not $state.hidden
    if ($form.Visible -ne $want) { $form.Visible = $want }
}

$timer = New-Object Windows.Forms.Timer
$timer.Interval = 1000
$timer.Add_Tick({
    $g = Game
    if ($g) { $state.seen = $true } elseif ($state.seen) { $form.Close(); return } else { return }
    $g.Refresh(); $win = $g.MainWindowHandle
    if ($win -eq [IntPtr]::Zero) { return }
    $state.owner = $win
    # The engine puts the frame rate in the window's title.
    if ($g.MainWindowTitle -match '([\d.]+) fps ([\d.]+) ms/frame (\d+) draws') { $s.fps = [double]$Matches[1]; $s.ms = [double]$Matches[2]; $s.draws = [int]$Matches[3] }
    try {
        if ($state.lastCpu) { $s.gameCpu = ($g.TotalProcessorTime - $state.lastCpu).TotalSeconds / ((Get-Date) - $state.lastAt).TotalSeconds / $cores * 100 }
        $state.lastCpu = $g.TotalProcessorTime; $state.lastAt = Get-Date
        $s.gameRam = $g.WorkingSet64 / 1GB; $s.gameCommit = $g.PrivateMemorySize64 / 1GB
    } catch {}
    if ($pc) { try { $s.pcCpu = $pc.NextValue() } catch {} }
    $ram = [HudWin]::Ram(); $s.ram = $ram[0]; $s.ramAll = $ram[1]
    # The card every other second: these questions take a tenth of a second each.
    $state.tick += 1
    if ($state.tick % 2 -eq 1) {
        $smi = $false
        try {
            $v = (nvidia-smi --query-gpu=memory.used,memory.total,utilization.gpu --format=csv,noheader,nounits) -split ',\s*'
            $s.vramUsed = [double]$v[0] / 1024; $s.vramAll = [double]$v[1] / 1024; $s.gpu = [double]$v[2]; $smi = $true
        } catch {}
        try {
            $all = Get-CimInstance Win32_PerfRawData_GPUPerformanceCounters_GPUProcessMemory
            $vm = $all | Where-Object Name -like "pid_$($g.Id)_*"
            $s.gameVram = ($vm | Measure-Object DedicatedUsage -Sum).Sum / 1GB; $s.gameShared = ($vm | Measure-Object SharedUsage -Sum).Sum / 1GB
            # The other program holding the most of the card, named when it is full (a python tool held
            # 3.9 GB of a 16 GB card while the game ran, 2026-11-27: "a VRAM leak" that was not the game's).
            $top = $all | Where-Object { $_.Name -notlike "pid_$($g.Id)_*" } | Group-Object { ($_.Name -split '_')[1] } |
                Sort-Object { ($_.Group | Measure-Object DedicatedUsage -Sum).Sum } -Descending | Select-Object -First 1
            $s.topVram = if ($top) { ($top.Group | Measure-Object DedicatedUsage -Sum).Sum / 1GB } else { 0 }
            $s.topName = if ($top) { (Get-Process -Id ([int]$top.Name) -ErrorAction SilentlyContinue).ProcessName }
            # No nvidia-smi (an AMD or Intel card): Windows' own counters, of the card the game draws on
            # (issue 3: "-%" and "- of - GB" on an RX 9070 XT). They agree with nvidia-smi on an RTX 5080.
            if (-not $smi -and $vm) {
                $luid = (($vm | Sort-Object DedicatedUsage -Descending | Select-Object -First 1).Name -split '_')[2..4] -join '_'
                $mem = Get-CimInstance Win32_PerfRawData_GPUPerformanceCounters_GPUAdapterMemory | Where-Object Name -like "$luid*"
                $s.vramUsed = ($mem | Measure-Object DedicatedUsage -Sum).Sum / 1GB
                $engines = Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine | Where-Object { $_.Name -like "*$($luid)_*engtype_3D" }
                $s.gpu = [math]::Min(100, ($engines | Measure-Object UtilizationPercentage -Sum).Sum)
                $s.vramAll = $cardAll
            }
        } catch {}
    }
    # Which keys hold: the engine writes "keys layout=drive" or "walk" at each press of G.
    if ($Log -and (Test-Path $Log)) {
        try {
            $fs = [IO.File]::Open($Log, 'Open', 'Read', 'ReadWrite')
            if ($fs.Length -lt $state.logAt) { $state.logAt = 0 }
            [void]$fs.Seek($state.logAt, 'Begin'); $new = (New-Object IO.StreamReader($fs)).ReadToEnd(); $state.logAt = $fs.Length; $fs.Close()
            $last = [regex]::Matches($new, 'binds: keys layout="?(walk|drive)') | Select-Object -Last 1
            if ($last) { $state.keys = if ($last.Groups[1].Value -eq "drive") { "driving" } else { "on foot" } }
        } catch {}
    }
    Place $win
    $form.Invalidate()
})
$timer.Start()
# Ten times a second: the panel follows the game's window, and F8 (while the game is in
# front) hides or shows it; read often so that a short press counts.
$fast = New-Object Windows.Forms.Timer
$fast.Interval = 100
$fast.Add_Tick({
    if ($state.owner -eq [IntPtr]::Zero) { return }
    $down = [HudWin]::GetAsyncKeyState(0x77) -lt 0
    if ($down -and -not $state.f8 -and [HudWin]::GetForegroundWindow() -eq $state.owner) { $state.hidden = -not $state.hidden }
    $state.f8 = $down
    Place $state.owner
})
$fast.Start()
[Windows.Forms.Application]::Run($form)

# In-game panel for a Vilocity GTA VI run: the frame rate, the upscaler and frame
# generation in use, and what the game and the PC use, in the upper left corner of the
# game's window.
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
$s = @{ fps = $null; ms = $null; draws = $null; gameCpu = $null; gameGpu = $null; vramUsed = $null; vramAll = $null
        gameVram = $null; gameShared = $null; gameRam = $null; gameCommit = $null; ram = $null; ramAll = $null
        # From the game's log: the upscaler (DLSS or FSR), its mode and sizes, and frame generation.
        up = $null; upMode = $null; upFrom = $null; upTo = $null; fg = $null; fgTimes = 0 }
$state = @{ logAt = 0L; id = 0; owner = [IntPtr]::Zero; lastCpu = $null; lastAt = $null; seen = $false; hidden = $false; f8 = $false; tick = 0 }
# The card's memory in GB, for cards without nvidia-smi (the largest the driver reports).
$cardAll = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*' -ErrorAction SilentlyContinue |
    ForEach-Object { [double]$_.'HardwareInformation.qwMemorySize' } | Measure-Object -Maximum).Maximum / 1GB

$PW = 270; $PH = 212
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
$nvidia = [Drawing.Color]::FromArgb(118, 185, 0); $amd = [Drawing.Color]::FromArgb(255, 140, 66)
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
    $row = { param($y, $what, $says, $part, $whole, $inner, $tint = $white, $warn = $true)
        $g.DrawString($what, $name, (& $brush $grey), 13, $y + 1)
        $g.DrawString($says, $value, (& $brush $tint), 62, $y)
        if ($null -ne $part -and $whole) {
            $g.FillRectangle((& $brush $track), 62, $y + 16, ($PW - 74), 3)
            $share = [math]::Min(1.0, [double]$part / [double]$whole)
            $g.FillRectangle((& $brush $(if ($warn -and $share -gt 0.92) { $bad } else { $brand })), 62, $y + 16, [int](($PW - 74) * $share), 3)
            if ($null -ne $inner) { $g.FillRectangle((& $brush $brand), 62, $y + 16, [int](($PW - 74) * [math]::Min(1.0, [double]$inner / [double]$whole)), 3) }
        }
    }
    # The game's own use only (the user's choice, 2026-11-28): of all the processor, of the card's
    # busiest engine, of the card's memory and of the PC's.
    & $row 44 "CPU" ("{0}%" -f (N $s.gameCpu "N0")) $s.gameCpu 100 $null $white $false
    & $row 66 "GPU" ("{0}%" -f (N $s.gameGpu "N0")) $s.gameGpu 100 $null $white $false
    # A full card: Windows moves the game's memory to the PC's and the card's frame takes 60-120 ms
    # (4-7 fps at 1440p with 5.6 GB held by others, 2026-11-27; 1080p needs some 5 GB less), so
    # then what the other programs hold is said, in red.
    $others = if ($null -ne $s.vramUsed -and $null -ne $s.gameVram) { $s.vramUsed - $s.gameVram } else { 0 }
    $full = $null -ne $s.vramUsed -and $s.vramAll -gt 0 -and ($s.vramUsed / $s.vramAll) -gt 0.93
    if ($full) {
        # (Short enough for the panel: "CARD FULL: others 7.3 GB (vramhold 3.0)" ran past its edge.)
        $text = if ($s.topName -and $s.topVram -ge 1) { "CARD FULL: {0} {1} GB" -f $s.topName.Substring(0, [math]::Min(10, $s.topName.Length)), (N $s.topVram) } else { "CARD FULL: others {0} GB" -f (N $others) }
        & $row 88 "VIDEO" $text $s.gameVram $s.vramAll $null $bad
    }
    else { & $row 88 "VIDEO" ("{0} of {1} GB   + {2} shared" -f (N $s.gameVram), (N $s.vramAll), (N $s.gameShared)) $s.gameVram $s.vramAll $null }
    & $row 110 "RAM" ("{0} of {1} GB" -f (N $s.gameRam), (N $s.ramAll "N0")) $s.gameRam $s.ramAll $null
    # A thin line between what the PC uses and the picture, and another above the keys.
    $g.FillRectangle((& $brush $track), 13, 134, ($PW - 26), 1)
    $g.FillRectangle((& $brush $track), 13, 180, ($PW - 26), 1)
    # The upscaler: which (DLSS in NVIDIA's green, FSR in orange), its mode, and the game's render
    # size to the output's ("game's own": no DLSS or FSR call seen, the console renderer's own size).
    $tint = if ($s.up -eq "DLSS") { $nvidia } elseif ($s.up -eq "FSR") { $amd } else { $white }
    $sizes = if ($s.upFrom) { "   {0}p to {1}p" -f $s.upFrom, $s.upTo } else { "" }
    & $row 140 "SCALE" $(if ($s.up) { "{0} {1}{2}" -f $s.up, $s.upMode, $sizes } else { "game's own" }) $null $null $null $tint
    # Frame generation: which, how many frames shown for each of the game's, and about how many reach the screen.
    $shown = if ($s.fgTimes -gt 1 -and $null -ne $s.fps) { "   ~{0} fps shown" -f (N ($s.fps * $s.fgTimes) "N0") } else { "" }
    $fgTint = if ($s.fgTimes -le 1) { $white } elseif ($s.fg -like 'DLSS*') { $nvidia } else { $amd }
    & $row 158 "FRAMES" $(if ($s.fg) { "{0}{1}" -f $s.fg, $shown } else { "no frame generation" }) $null $null $null $fgTint
    # A key as a keycap and what it does, from x; gives the x after it.
    $cap = { param($x, $y, $key, $does)
        $w = [int]$g.MeasureString($key, $name).Width + 5
        $p = New-Object Drawing.Drawing2D.GraphicsPath; $r = 6
        $p.AddArc($x, $y, $r, $r, 180, 90); $p.AddArc($x + $w - $r, $y, $r, $r, 270, 90)
        $p.AddArc($x + $w - $r, $y + 15 - $r, $r, $r, 0, 90); $p.AddArc($x, $y + 15 - $r, $r, $r, 90, 90); $p.CloseFigure()
        $g.FillPath((& $brush $track), $p); $g.DrawPath((New-Object Drawing.Pen $grey), $p)
        $g.DrawString($key, $name, (& $brush $white), ($x + 3), ($y + 1))
        $g.DrawString($does, $value, (& $brush $grey), ($x + $w + 3), ($y - 1))
        $x + $w + 12 + [int]$g.MeasureString($does, $value).Width
    }
    # The keys that are Vilocity's own.
    $x = & $cap 13 188 "F1" "settings"; $x = & $cap $x 188 "F8" "hide"; [void](& $cap $x 188 "F11" "full screen")
})

# The game's process and window: looked for until found, then kept.
function Game {
    if ($state.id) { $p = Get-Process -Id $state.id -ErrorAction SilentlyContinue; if ($p) { return $p }; $state.id = 0 }
    $game = Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" | Where-Object CommandLine -like '*native run*' | Select-Object -First 1
    if ($game) { $state.id = $game.ProcessId; Get-Process -Id $game.ProcessId -ErrorAction SilentlyContinue }
}
# In the upper left corner of the game's picture; shown with the game's window.
function Place($win) {
    $r = New-Object HudWin+RECT; $p = New-Object HudWin+POINT
    $shown = [HudWin]::IsWindowVisible($win) -and -not [HudWin]::IsIconic($win) -and [HudWin]::GetClientRect($win, [ref]$r) -and [HudWin]::ClientToScreen($win, [ref]$p)
    if ($shown) {
        $at = New-Object Drawing.Point(($p.X + 14), ($p.Y + 14)); if ($form.Location -ne $at) { $form.Location = $at }
        # Only where the game's picture is what the user sees there (the panel is over all
        # windows, as it must be to be over the game's without belonging to it): asked
        # just right of and just under the panel, where the panel itself is not found.
        $shown = [HudWin]::Seen($win, ($at.X + $PW + 5), ($at.Y + $PH / 2)) -and [HudWin]::Seen($win, ($at.X + $PW / 2), ($at.Y + $PH + 5))
    }
    $want = $shown -and -not $state.hidden
    if ($form.Visible -ne $want) { $form.Visible = $want }
}

# The upscaler and frame generation, from the lines the engine writes: DLSS's ("upscaling DLSS on",
# "DLSS: upscaling recorded", "DLSS Frame Generation switched") and FSR's ("FSR upscaler: the
# game's upscale", "FSR frame generation: on"). The last of each kind holds.
function Read-Upscaling([string]$text) {
    $title = { param($w) (Get-Culture).TextInfo.ToTitleCase($w.ToLower()) }
    foreach ($m in [regex]::Matches($text, 'upscaling\s+DLSS on, frame generation ([^,\r\n]+)(?:, scene at the (\w+) scale)?')) {
        $s.up = "DLSS"; if ($m.Groups[2].Success) { $s.upMode = & $title $m.Groups[2].Value }
        if ($m.Groups[1].Value -like 'asked*') { $s.fg = "DLSS's: not available"; $s.fgTimes = 0 }
    }
    foreach ($m in [regex]::Matches($text, 'DLSS: upscaling recorded.*?render=\[(\d+), (\d+)\] output=\[(\d+), (\d+)\] mode="(\w+)"')) {
        $s.up = "DLSS"; $s.upFrom = [int]$m.Groups[2].Value; $s.upTo = [int]$m.Groups[4].Value; $s.upMode = & $title $m.Groups[5].Value
    }
    $last = [regex]::Matches($text, 'DLSS Frame Generation switched.*?on=(true|false) times=(\d+)') | Select-Object -Last 1
    if ($last) {
        if ($last.Groups[1].Value -eq 'true') { $s.fgTimes = [int]$last.Groups[2].Value; $s.fg = "DLSS {0}x" -f $s.fgTimes } else { $s.fg = "DLSS's: off now"; $s.fgTimes = 0 }
    }
    if ($text -match 'upscaling\s+AMD FSR frame generation') { $s.up = "FSR"; $s.upMode = "Quality" }
    foreach ($m in [regex]::Matches($text, "FSR upscaler: the game's upscale render=\[(\d+), (\d+)\] output=\[(\d+), (\d+)\]")) {
        if ($s.up -eq "DLSS") { continue }
        $s.up = "FSR"; $s.upFrom = [int]$m.Groups[2].Value; $s.upTo = [int]$m.Groups[4].Value
        if (-not $s.upMode -and $s.upFrom -gt 0) {
            # FSR's modes by their scale: 1.5 Quality, 1.7 Balanced, 2 Performance, 3 Ultra Performance.
            $ratio = $s.upTo / $s.upFrom
            $s.upMode = @(@(1.0, 'Native AA'), @(1.5, 'Quality'), @(1.7, 'Balanced'), @(2.0, 'Performance'), @(3.0, 'Ultra Performance')) |
                Sort-Object { [math]::Abs($_[0] - $ratio) } | Select-Object -First 1 | ForEach-Object { $_[1] }
        }
    }
    if ($text -match 'FSR frame generation: on') { $s.fg = "FSR 2x"; $s.fgTimes = 2 }
    if ($text -match 'FSR frame generation: (the SDK made no swap chain|its context could not be made)') { $s.fg = "FSR's: refused (see the log)"; $s.fgTimes = 0 }
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
    $s.ramAll = ([HudWin]::Ram())[1]
    # The card every other second: these questions take a tenth of a second each.
    $state.tick += 1
    if ($state.tick % 2 -eq 1) {
        $smi = $false
        try {
            $v = (nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader,nounits) -split ',\s*'
            $s.vramUsed = [double]$v[0] / 1024; $s.vramAll = [double]$v[1] / 1024; $smi = $true
        } catch {}
        try {
            $all = Get-CimInstance Win32_PerfRawData_GPUPerformanceCounters_GPUProcessMemory
            $vm = $all | Where-Object Name -like "pid_$($g.Id)_*"
            $s.gameVram = ($vm | Measure-Object DedicatedUsage -Sum).Sum / 1GB; $s.gameShared = ($vm | Measure-Object SharedUsage -Sum).Sum / 1GB
            # The game's own share of the card: its busiest kind of engine (3D, compute, copy...), as Task Manager counts it.
            $mine = Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine | Where-Object Name -like "pid_$($g.Id)_*"
            $s.gameGpu = [math]::Min(100, [double](($mine | Group-Object { ($_.Name -split 'engtype_')[1] } | ForEach-Object { ($_.Group | Measure-Object UtilizationPercentage -Sum).Sum }) | Measure-Object -Maximum).Maximum)
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
                $s.vramAll = $cardAll
            }
        } catch {}
    }
    # The upscaler and frame generation, from what the engine has written since the last look.
    if ($Log -and (Test-Path $Log)) {
        try {
            $fs = [IO.File]::Open($Log, 'Open', 'Read', 'ReadWrite')
            if ($fs.Length -lt $state.logAt) { $state.logAt = 0 }
            [void]$fs.Seek($state.logAt, 'Begin'); $new = (New-Object IO.StreamReader($fs)).ReadToEnd(); $state.logAt = $fs.Length; $fs.Close()
            Read-Upscaling $new
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

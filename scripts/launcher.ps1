# The Vilocity launcher: one window to check the PC, pick the game folder, set the
# options and play GTA VI. Start it with "Vilocity Launcher.cmd" in the repository's
# folder. It runs scripts\play-gta6.ps1, which holds the engine's settings; what is
# chosen here is kept in %LOCALAPPDATA%\vilocity\launcher.json.
#   scripts\launcher.ps1 [-Shot <file.png>]   (-Shot: draw the window to a picture and leave)
param([string]$Shot)
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Windows.Forms

# NVIDIA Smooth Motion (the driver's own frame generation, RTX 40 and 50 series) for one
# executable, through the driver's settings (NVAPI DRS): setting 0xB0D384C0 (0 off, 1 on)
# and 0xB0CC0875 (1: Direct3D 12) in the driver profile that holds the executable, made
# when none does. Nothing else of the driver's is touched. (The ids and the calls as in
# LooperSalty/AnyXboxX's launcher and NVIDIA Profile Inspector.)
Add-Type @'
using System; using System.Runtime.InteropServices; using System.Text;
public static class NvSmooth {
    [DllImport("nvapi64.dll", EntryPoint = "nvapi_QueryInterface", CallingConvention = CallingConvention.Cdecl)] static extern IntPtr Query(uint id);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int NoArgs();
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int Out1(out IntPtr a);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int On1(IntPtr session);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int FindApp(IntPtr session, IntPtr name, out IntPtr profile, IntPtr app);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int FindProfile(IntPtr session, IntPtr name, out IntPtr profile);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int CreateProfile(IntPtr session, IntPtr info, out IntPtr profile);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int CreateApp(IntPtr session, IntPtr profile, IntPtr app);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int GetSetting(IntPtr session, IntPtr profile, uint id, IntPtr setting, ref uint extra);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int SetSetting(IntPtr session, IntPtr profile, IntPtr setting, uint a, uint b);
    // NVAPI's structures by their sizes (its version words: the size, and the version above bit 16).
    const int Name = 4096, SettingSize = 12320, AppSize = 20492, ProfileSize = 4116;
    const uint Enable = 0xB0D384C0, Apis = 0xB0CC0875;
    static T F<T>(uint id, uint older) { IntPtr p = Query(id); if (p == IntPtr.Zero && older != 0) p = Query(older); if (p == IntPtr.Zero) throw new Exception("the driver has no call " + id.ToString("X8")); return (T)(object)Marshal.GetDelegateForFunctionPointer(p, typeof(T)); }
    static void Check(int status, string what) { if (status != 0) throw new Exception(what + " failed (" + status + ")"); }
    static IntPtr Zeroed(int size) { IntPtr p = Marshal.AllocHGlobal(size); for (int i = 0; i < size; i += 4) Marshal.WriteInt32(p, i, 0); return p; }
    static void Put(IntPtr at, int offset, string text) { byte[] b = Encoding.Unicode.GetBytes(text); Marshal.Copy(b, 0, IntPtr.Add(at, offset), Math.Min(b.Length, Name - 2)); }
    static IntPtr Open() { Check(F<NoArgs>(0x0150E828, 0)(), "starting NVAPI"); IntPtr s; Check(F<Out1>(0x0694D52E, 0)(out s), "opening the driver's settings"); Check(F<On1>(0x375DBD6B, 0)(s), "loading the driver's settings"); return s; }
    static void Close(IntPtr s) { try { F<On1>(0xDAD9CFF8, 0)(s); } catch { } }
    static IntPtr ProfileOf(IntPtr session, string exe) {
        IntPtr app = Zeroed(AppSize), name = Zeroed(Name), profile;
        try { Marshal.WriteInt32(app, 0, AppSize | 4 << 16); Put(name, 0, exe);
            int st = F<FindApp>(0xEEE566B2, 0)(session, name, out profile, app);
            if (st == -166) return IntPtr.Zero; Check(st, "looking for " + exe); return profile; }
        finally { Marshal.FreeHGlobal(app); Marshal.FreeHGlobal(name); }
    }
    static IntPtr Dword(uint id, uint value) { IntPtr s = Zeroed(SettingSize); Marshal.WriteInt32(s, 0, SettingSize | 1 << 16); Marshal.WriteInt32(s, 4 + Name, unchecked((int)id)); Marshal.WriteInt32(s, 4 + Name + 20 + 4100, unchecked((int)value)); return s; }
    // 1: on for `exe`, 0: off, -1: not set (no profile holds it, or the profile says nothing).
    public static int Get(string exe) {
        IntPtr session = Open();
        try { IntPtr profile = ProfileOf(session, exe); if (profile == IntPtr.Zero) return -1;
            IntPtr s = Dword(Enable, 0); uint extra = 0;
            try { int st = F<GetSetting>(0xEA99498D, 0x73BF8338)(session, profile, Enable, s, ref extra); if (st == -160) return -1; Check(st, "reading the setting");
                return Marshal.ReadInt32(s, 4 + Name + 20 + 4100) != 0 ? 1 : 0; }
            finally { Marshal.FreeHGlobal(s); } }
        finally { Close(session); }
    }
    // Turns it on or off for `exe`, in the profile named `profileName` if none holds the executable yet.
    public static string Set(string exe, string profileName, bool on) {
        IntPtr session = Open(); string made = "";
        try { IntPtr profile = ProfileOf(session, exe);
            if (profile == IntPtr.Zero) {
                IntPtr name = Zeroed(Name); Put(name, 0, profileName);
                int found = F<FindProfile>(0x7E4A9A0B, 0)(session, name, out profile); Marshal.FreeHGlobal(name);
                if (found == -163) { IntPtr info = Zeroed(ProfileSize); Marshal.WriteInt32(info, 0, ProfileSize | 1 << 16); Put(info, 4, profileName); Marshal.WriteInt32(info, 4 + Name, 1);
                    try { Check(F<CreateProfile>(0xCC176068, 0)(session, info, out profile), "making the profile"); } finally { Marshal.FreeHGlobal(info); } made = "profile made, "; }
                else Check(found, "looking for the profile");
                IntPtr app = Zeroed(AppSize); Marshal.WriteInt32(app, 0, AppSize | 4 << 16); Put(app, 8, exe);
                try { Check(F<CreateApp>(0x4347A9DE, 0)(session, profile, app), "adding " + exe); } finally { Marshal.FreeHGlobal(app); }
            }
            SetSetting write = F<SetSetting>(0x8A2CF5F5, 0x577DD202);
            IntPtr s = Dword(Enable, on ? 1u : 0u); try { Check(write(session, profile, s, 0, 0), "writing the setting"); } finally { Marshal.FreeHGlobal(s); }
            if (on) { s = Dword(Apis, 1); try { Check(write(session, profile, s, 0, 0), "writing the setting's APIs"); } finally { Marshal.FreeHGlobal(s); } }
            Check(F<On1>(0xFCBC7E14, 0)(session), "saving the driver's settings");
            return made + "Smooth Motion " + (on ? "on" : "off");
        } finally { Close(session); }
    }
}
'@

$root = Split-Path $PSScriptRoot
$home_ = Join-Path $env:LOCALAPPDATA "vilocity"
$file = Join-Path $home_ "launcher.json"
$exe = Join-Path $root "target\release\vilocity.exe"

# What the window remembers, with what it starts from. (The game folder: the user's
# variable as it is now, not as this window's parent had it.)
$known = [Environment]::GetEnvironmentVariable('VILOCITY_GTA6_DIR', 'User'); if (-not $known) { $known = $env:VILOCITY_GTA6_DIR }
$set = @{ game = $known; mode = 0; cap = 0; sharp = 0.0; fsr = $true; hud = $true; sandbox = $true; lights = $false; leave = $false; extra = "" }
if (Test-Path $file) {
    try { $old = Get-Content $file -Raw | ConvertFrom-Json; foreach ($k in @($set.Keys)) { if ($null -ne $old.$k) { $set[$k] = $old.$k } } } catch {}
}
# A remembered folder that is gone (the game was moved): the user's variable, if that one is there.
$there = { param($d) $d -and (Test-Path (Join-Path $d "game_scarlett_master.exe")) }
if (-not (& $there $set.game) -and (& $there $known)) { $set.game = $known }

[xml]$xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Vilocity" Width="960" Height="860" MinWidth="880" MinHeight="800" WindowStartupLocation="CenterScreen"
        Background="#0B0D12" Foreground="#E6ECF5" FontFamily="Segoe UI" FontSize="13">
  <Window.Resources>
    <Style TargetType="TextBlock"><Setter Property="Foreground" Value="#E6ECF5"/></Style>
    <Style x:Key="Dim" TargetType="TextBlock"><Setter Property="Foreground" Value="#8B93A7"/><Setter Property="FontSize" Value="12"/><Setter Property="TextWrapping" Value="Wrap"/></Style>
    <Style x:Key="Head" TargetType="TextBlock"><Setter Property="Foreground" Value="#8B93A7"/><Setter Property="FontSize" Value="11"/><Setter Property="FontWeight" Value="SemiBold"/><Setter Property="Margin" Value="0,0,0,10"/></Style>
    <Style x:Key="Label" TargetType="TextBlock"><Setter Property="Foreground" Value="#E6ECF5"/><Setter Property="Margin" Value="0,0,0,6"/></Style>
    <Style TargetType="Button">
      <Setter Property="Foreground" Value="#E6ECF5"/><Setter Property="Background" Value="#1D2230"/><Setter Property="Padding" Value="14,7"/><Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="Button">
        <Border x:Name="b" Background="{TemplateBinding Background}" CornerRadius="6" Padding="{TemplateBinding Padding}" BorderThickness="1" BorderBrush="Transparent">
          <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/></Border>
        <ControlTemplate.Triggers>
          <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="b" Property="Opacity" Value="0.85"/></Trigger>
          <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#E6ECF5"/></Trigger>
          <Trigger Property="IsEnabled" Value="False"><Setter TargetName="b" Property="Opacity" Value="0.45"/></Trigger>
        </ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
    </Style>
    <Style x:Key="Pill" TargetType="RadioButton">
      <Setter Property="Foreground" Value="#E6ECF5"/><Setter Property="Margin" Value="0,0,6,0"/><Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="RadioButton">
        <Border x:Name="b" Background="#1D2230" CornerRadius="6" Padding="14,6" BorderThickness="1" BorderBrush="Transparent"><ContentPresenter/></Border>
        <ControlTemplate.Triggers>
          <Trigger Property="IsChecked" Value="True"><Setter TargetName="b" Property="Background" Value="#7C5CFF"/></Trigger>
          <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#E6ECF5"/></Trigger>
        </ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
    </Style>
    <Style TargetType="CheckBox"><Setter Property="Foreground" Value="#E6ECF5"/><Setter Property="Margin" Value="0,0,0,10"/></Style>
    <Style TargetType="TextBox"><Setter Property="Background" Value="#0B0D12"/><Setter Property="Foreground" Value="#E6ECF5"/><Setter Property="BorderBrush" Value="#2A3142"/>
      <Setter Property="Padding" Value="8,6"/><Setter Property="CaretBrush" Value="#E6ECF5"/></Style>
  </Window.Resources>
  <Grid Margin="28,22,28,22">
    <Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
    <StackPanel Orientation="Horizontal" Margin="0,0,0,18">
      <Image x:Name="Logo" Width="46" Height="46" Margin="0,0,12,0" RenderOptions.BitmapScalingMode="HighQuality"/>
      <StackPanel VerticalAlignment="Center"><TextBlock Text="Vilocity" FontSize="22" FontWeight="SemiBold"/><TextBlock Style="{StaticResource Dim}" Text="Grand Theft Auto VI, running natively on Windows"/></StackPanel>
    </StackPanel>
    <Border Grid.Row="1" Background="#151926" CornerRadius="10" Padding="16,14" Margin="0,0,0,14">
      <Grid><Grid.ColumnDefinitions><ColumnDefinition Width="Auto"/><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
        <Border x:Name="CoverBox" Width="64" Height="64" CornerRadius="8" Margin="0,0,14,0" Visibility="Collapsed"><Border.Background><ImageBrush x:Name="Cover" Stretch="UniformToFill"/></Border.Background></Border>
        <StackPanel Grid.Column="1" VerticalAlignment="Center"><TextBlock x:Name="GameName" Style="{StaticResource Head}" Text="GAME FOLDER"/><TextBox x:Name="Game" ToolTip="The folder that holds game_scarlett_master.exe"/></StackPanel>
        <Button x:Name="Browse" Grid.Column="2" Content="Browse..." Margin="10,0,0,0" VerticalAlignment="Bottom"/></Grid>
    </Border>
    <Grid Grid.Row="2"><Grid.ColumnDefinitions><ColumnDefinition Width="5*"/><ColumnDefinition Width="14"/><ColumnDefinition Width="6*"/></Grid.ColumnDefinitions>
      <Border Background="#151926" CornerRadius="10" Padding="16,14">
        <DockPanel><TextBlock DockPanel.Dock="Top" Style="{StaticResource Head}" Text="THIS PC"/>
          <Button x:Name="Recheck" DockPanel.Dock="Bottom" Content="Check again" HorizontalAlignment="Left" Margin="0,8,0,0"/>
          <ScrollViewer VerticalScrollBarVisibility="Auto"><StackPanel x:Name="Checks"/></ScrollViewer></DockPanel>
      </Border>
      <Border Grid.Column="2" Background="#151926" CornerRadius="10" Padding="16,14">
        <StackPanel>
          <TextBlock Style="{StaticResource Head}" Text="OPTIONS"/>
          <TextBlock Style="{StaticResource Label}" Text="Picture"/>
          <StackPanel Orientation="Horizontal"><RadioButton x:Name="Mode0" GroupName="mode" Style="{StaticResource Pill}" Content="Fast"/><RadioButton x:Name="Mode1" GroupName="mode" Style="{StaticResource Pill}" Content="Quality"/></StackPanel>
          <TextBlock x:Name="ModeText" Style="{StaticResource Dim}" Margin="0,6,0,14"/>
          <TextBlock Style="{StaticResource Label}" Text="Frame rate"/>
          <StackPanel Orientation="Horizontal" Margin="0,0,0,14"><RadioButton x:Name="Cap0" GroupName="cap" Style="{StaticResource Pill}" Content="No limit"/><RadioButton x:Name="Cap1" GroupName="cap" Style="{StaticResource Pill}" Content="At most 60"/><RadioButton x:Name="Cap2" GroupName="cap" Style="{StaticResource Pill}" Content="At most 30"/></StackPanel>
          <StackPanel Orientation="Horizontal"><TextBlock Text="Sharpening"/><TextBlock x:Name="SharpText" Style="{StaticResource Dim}" Margin="8,1,0,0"/></StackPanel>
          <Slider x:Name="Sharp" Minimum="0" Maximum="1" TickFrequency="0.1" IsSnapToTickEnabled="True" Margin="0,6,0,14"/>
          <CheckBox x:Name="Fsr"><TextBlock TextWrapping="Wrap" Text="AMD's PC upscaler, if its DLL is installed (a little faster)"/></CheckBox>
          <CheckBox x:Name="Hud"><TextBlock TextWrapping="Wrap" Text="Frame rate panel in the game (F8 hides it)"/></CheckBox>
          <CheckBox x:Name="Sandbox"><TextBlock TextWrapping="Wrap" Text="Sandbox: no network, none of your files (recommended)"/></CheckBox>
          <CheckBox x:Name="Lights" ToolTip="Runs five of the game's programs that are left out on 32-lane cards (NVIDIA): light tiles, visibility and light through fog"><TextBlock TextWrapping="Wrap" Text="Lamp light and lit fog at night (on trial: slower)"/></CheckBox>
          <CheckBox x:Name="Smooth" ToolTip="Written into the NVIDIA driver's profile for vilocity.exe when you press PLAY; nothing else of the driver is changed"><TextBlock TextWrapping="Wrap" Text="NVIDIA Smooth Motion: the driver shows a made-up picture between every two (twice the frame rate shown; RTX 40 and 50)"/></CheckBox>
          <CheckBox x:Name="Leave"><TextBlock TextWrapping="Wrap" Text="Close this window when the game starts"/></CheckBox>
          <TextBlock Style="{StaticResource Label}" Text="More settings" Margin="0,4,0,6"/>
          <TextBox x:Name="Extra" ToolTip="NAME=VALUE; NAME=VALUE. For example: VILOCITY_AMBIENT=1.0; VILOCITY_THIN_THREADS=12"/>
        </StackPanel>
      </Border>
    </Grid>
    <Grid Grid.Row="3" Margin="0,16,0,0"><Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
      <StackPanel VerticalAlignment="Center">
        <TextBlock x:Name="Status" FontSize="14" TextWrapping="Wrap"/>
        <StackPanel Orientation="Horizontal" Margin="0,10,0,0">
          <Button x:Name="Keys" Content="Keys" Margin="0,0,8,0"/><Button x:Name="Saves" Content="Saves" Margin="0,0,8,0"/><Button x:Name="Logs" Content="Logs" Margin="0,0,8,0"/><Button x:Name="Build" Content="Build the engine" ToolTip="cargo build --release -p vilocity, in a window of its own"/></StackPanel>
      </StackPanel>
      <Button x:Name="Play" Grid.Column="1" Content="PLAY" FontSize="18" FontWeight="Bold" Padding="46,14" Background="#7C5CFF" VerticalAlignment="Center" IsDefault="True"/>
    </Grid>
  </Grid>
</Window>
'@
$w = [Windows.Markup.XamlReader]::Load((New-Object System.Xml.XmlNodeReader $xaml))
$ui = @{}; foreach ($n in 'Logo', 'CoverBox', 'Cover', 'GameName', 'Game', 'Browse', 'Checks', 'Recheck', 'Mode0', 'Mode1', 'ModeText', 'Cap0', 'Cap1', 'Cap2', 'Sharp', 'SharpText', 'Fsr', 'Hud', 'Sandbox', 'Lights', 'Smooth', 'Leave', 'Extra', 'Status', 'Keys', 'Saves', 'Logs', 'Build', 'Play') { $ui[$n] = $w.FindName($n) }
# The logo, in the window, its title bar and its taskbar button.
$logo = Join-Path $root "assets\logo-256.png"
if (Test-Path $logo) { $picture = New-Object Windows.Media.Imaging.BitmapImage ([Uri]$logo); $ui.Logo.Source = $picture; $w.Icon = $picture }

$ui.Game.Text = [string]$set.game
$ui["Mode$([math]::Min([int]$set.mode, 1))"].IsChecked = $true; $ui["Cap$([math]::Min([int]$set.cap, 2))"].IsChecked = $true
$ui.Sharp.Value = [double]$set.sharp; $ui.Fsr.IsChecked = [bool]$set.fsr; $ui.Hud.IsChecked = [bool]$set.hud
$ui.Sandbox.IsChecked = [bool]$set.sandbox; $ui.Lights.IsChecked = [bool]$set.lights; $ui.Leave.IsChecked = [bool]$set.leave; $ui.Extra.Text = [string]$set.extra
# Smooth Motion as the driver holds it now for vilocity.exe; no NVIDIA driver: no such option.
$smooth = @{ driver = $null }
try { $smooth.driver = [NvSmooth]::Get('vilocity.exe') -eq 1; $ui.Smooth.IsChecked = $smooth.driver } catch { $ui.Smooth.Visibility = 'Collapsed' }
function Mode { if ($ui.Mode1.IsChecked) { 1 } else { 0 } }
function Cap { if ($ui.Cap2.IsChecked) { 2 } elseif ($ui.Cap1.IsChecked) { 1 } else { 0 } }
$modeText = { $ui.ModeText.Text = if ((Mode) -eq 1) { "The game's Series X picture: more detail and pixels, about half the frame rate." } else { "The game's Series S picture: 1280x720 upscaled to 1920x1080, the fastest." } }
$ui.Mode0.Add_Checked($modeText); $ui.Mode1.Add_Checked($modeText); & $modeText
$sharpText = { $ui.SharpText.Text = if ($ui.Sharp.Value -le 0) { "off, as the game asks" } else { "{0:N1}" -f $ui.Sharp.Value } }
$ui.Sharp.Add_ValueChanged($sharpText); & $sharpText

function Save-Choices {
    New-Item -ItemType Directory -Force $home_ | Out-Null
    @{ game = $ui.Game.Text.Trim(); mode = (Mode); cap = (Cap); sharp = [math]::Round($ui.Sharp.Value, 1)
       fsr = [bool]$ui.Fsr.IsChecked; hud = [bool]$ui.Hud.IsChecked; sandbox = [bool]$ui.Sandbox.IsChecked; lights = [bool]$ui.Lights.IsChecked
       leave = [bool]$ui.Leave.IsChecked; extra = $ui.Extra.Text.Trim() } | ConvertTo-Json | Set-Content $file
}

# One line of the check list: a dot (green: fine, yellow: it works but, red: it will not start) and what was found.
function Add-Check([string]$level, [string]$text) {
    $colour = @{ ok = "#3DDC84"; warn = "#FACC15"; bad = "#F87171" }[$level]
    $row = New-Object Windows.Controls.DockPanel; $row.Margin = "0,0,0,9"
    $dot = New-Object Windows.Shapes.Ellipse; $dot.Width = 9; $dot.Height = 9; $dot.Margin = "0,5,10,0"; $dot.VerticalAlignment = "Top"
    $dot.Fill = (New-Object Windows.Media.BrushConverter).ConvertFromString($colour)
    $t = New-Object Windows.Controls.TextBlock; $t.Text = $text; $t.TextWrapping = "Wrap"
    [void]$row.Children.Add($dot); [void]$row.Children.Add($t); [void]$ui.Checks.Children.Add($row)
}

# Looks at the PC and the folders; whether the game can start.
function Test-Machine {
    $ui.Checks.Children.Clear(); $ready = $true
    $game = $ui.Game.Text.Trim().Trim('"')
    $ui.CoverBox.Visibility = 'Collapsed'; $ui.GameName.Text = "GAME FOLDER"
    if ($game -and (Test-Path (Join-Path $game "game_scarlett_master.exe"))) {
        Add-Check ok "Game folder: game_scarlett_master.exe is there"
        # The game's own name and logo, as its manifest names them (files of your copy).
        $manifest = Get-Content (Join-Path $game "MicrosoftGame.config") -Raw -ErrorAction SilentlyContinue
        if ($manifest -match 'DefaultDisplayName="([^"]+)"') { $ui.GameName.Text = $Matches[1].ToUpper() }
        if ($manifest -match 'Square150x150Logo="([^"]+)"' -and (Test-Path (Join-Path $game $Matches[1]))) {
            $cover = New-Object Windows.Media.Imaging.BitmapImage; $cover.BeginInit(); $cover.CacheOption = 'OnLoad'; $cover.UriSource = [Uri](Join-Path $game $Matches[1]); $cover.EndInit()
            $ui.Cover.ImageSource = $cover; $ui.CoverBox.Visibility = 'Visible'
        }
    }
    else { Add-Check bad "Game folder: no game_scarlett_master.exe in it. Pick the folder of your own copy of the game."; $ready = $false }
    if (Test-Path $exe) { Add-Check ok ("Engine: built {0:yyyy-MM-dd HH:mm}" -f (Get-Item $exe).LastWriteTime) }
    else { Add-Check bad "Engine: not built yet. In the repository's folder run: cargo build --release -p vilocity"; $ready = $false }
    try {
        $v = (nvidia-smi --query-gpu=name,memory.used,memory.total,driver_version --format=csv,noheader,nounits 2>$null) -split ',\s*'
        if ($v.Count -lt 4) { throw "no nvidia-smi" }
        $free = ([double]$v[2] - [double]$v[1]) / 1024
        $level = if ($free -lt 9) { "warn" } else { "ok" }
        Add-Check $level ("Graphics card: {0} (driver {4}), {1:N1} of {2:N0} GB of video memory free{3}" -f ($v[0] -replace 'NVIDIA GeForce ', ''), $free, ([double]$v[2] / 1024), $(if ($level -eq "warn") { ". The game wants about 11 GB: with less, textures load late and look soft." } else { "" }), $v[3])
        # Who else holds video memory: worth closing before playing.
        $mem = Get-CimInstance Win32_PerfRawData_GPUPerformanceCounters_GPUProcessMemory -ErrorAction SilentlyContinue | Where-Object { $_.DedicatedUsage -gt 1GB }
        foreach ($m in $mem) {
            $id = [int]($m.Name -replace '^pid_(\d+)_.*', '$1'); $p = Get-Process -Id $id -ErrorAction SilentlyContinue
            if ($p -and $p.ProcessName -notin 'vilocity', 'dwm') { Add-Check warn ("{0} is holding {1:N1} GB of video memory. Closing it gives the game sharper textures and steadier frames." -f $p.ProcessName, ($m.DedicatedUsage / 1GB)) }
        }
    } catch {
        $card = Get-CimInstance Win32_VideoController | Sort-Object AdapterRAM -Descending | Select-Object -First 1
        Add-Check ok ("Graphics card: {0}" -f $card.Name)
    }
    $os = Get-CimInstance Win32_OperatingSystem
    $ram = $os.TotalVisibleMemorySize / 1MB; $commit = $os.FreeVirtualMemory / 1MB
    if ($commit -lt 28) { Add-Check warn ("Memory: {0:N0} GB of RAM, {1:N0} GB free with the page file. The game takes about 26 GB: close other programs, or let Windows manage the page file." -f $ram, $commit) }
    else { Add-Check ok ("Memory: {0:N0} GB of RAM, {1:N0} GB free with the page file" -f $ram, $commit) }
    $drive = (Get-Item $env:LOCALAPPDATA).PSDrive
    if ($drive.Free -lt 30GB) { Add-Check warn ("Disk {0}: {1:N0} GB free. The shader cache and the page file want 30 GB." -f $drive.Name, ($drive.Free / 1GB)) }
    else { Add-Check ok ("Disk {0}: {1:N0} GB free" -f $drive.Name, ($drive.Free / 1GB)) }
    $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
    Add-Check ok ("Processor: {0}, {1} cores" -f ($cpu.Name.Trim() -replace '\s+', ' '), $cpu.NumberOfCores)
    $vc = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64' -ErrorAction SilentlyContinue
    if (-not $vc -or $vc.Installed -ne 1) { Add-Check warn "Visual C++ runtime (x64): not found. The game needs it: https://aka.ms/vs/17/release/vc_redist.x64.exe" }
    $ready
}

function Game-Process {
    $p = Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" | Where-Object CommandLine -like '*native run*' | Select-Object -First 1
    if ($p) { Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue }
}

$state = @{ ready = $false; started = $null; seen = $false }
$refresh = { $state.ready = Test-Machine; if (-not $state.started) { $ui.Play.IsEnabled = $state.ready; $ui.Status.Text = if ($state.ready) { "Ready." } else { "Not ready: see the red lines." } } }

$ui.Browse.Add_Click({
    $d = New-Object Windows.Forms.FolderBrowserDialog; $d.Description = "The folder that holds game_scarlett_master.exe"
    if ($ui.Game.Text -and (Test-Path $ui.Game.Text)) { $d.SelectedPath = $ui.Game.Text }
    if ($d.ShowDialog() -eq 'OK') { $ui.Game.Text = $d.SelectedPath; & $refresh }
})
$ui.Game.Add_LostFocus({ & $refresh })
$ui.Recheck.Add_Click({ & $refresh })
$ui.Keys.Add_Click({ $k = Join-Path $home_ "data\keys.txt"; if (Test-Path $k) { Start-Process notepad $k } else { $ui.Status.Text = "The keys file is made on the game's first start." } })
$ui.Saves.Add_Click({ $d = Join-Path $home_ "data"; New-Item -ItemType Directory -Force $d | Out-Null; Start-Process explorer $d })
$ui.Logs.Add_Click({ $d = Join-Path $home_ "logs"; New-Item -ItemType Directory -Force $d | Out-Null; Start-Process explorer $d })
$ui.Build.Add_Click({ Start-Process cmd -ArgumentList '/k', "cd /d `"$root`" && cargo build --release -p vilocity && echo. && echo Built. Close this window and press Check again." })

$ui.Play.Add_Click({
    if ($state.started) { Get-Process vilocity -ErrorAction SilentlyContinue | Stop-Process -Force; return }
    Save-Choices
    # Smooth Motion into the driver's profile for vilocity.exe, when the box differs from what the driver holds.
    $note = ""
    if ($ui.Smooth.Visibility -eq 'Visible' -and [bool]$ui.Smooth.IsChecked -ne $smooth.driver) {
        try { $note = " (" + [NvSmooth]::Set('vilocity.exe', 'Vilocity (vilocity.exe)', [bool]$ui.Smooth.IsChecked) + ")"; $smooth.driver = [bool]$ui.Smooth.IsChecked }
        catch { $note = " (Smooth Motion was not changed: $($_.Exception.InnerException.Message))" }
    }
    # The options as the play script's switches and the engine's variables; what "More settings" names comes last and wins.
    $vars = [ordered]@{
        VILOCITY_FRAME_INTERVAL_MS = @("0", "16.667", "33.333")[(Cap)]
        VILOCITY_FSR_SHARPNESS = ([math]::Round($ui.Sharp.Value, 1)).ToString([Globalization.CultureInfo]::InvariantCulture)
        VILOCITY_HUD = $(if ($ui.Hud.IsChecked) { "1" } else { "0" })
    }
    if (-not $ui.Fsr.IsChecked) { $vars.VILOCITY_FSR_DLL = "off" }
    if ($ui.Lights.IsChecked) {
        # The light tiles, visibility and light through fog (wave64 programs in the wide form).
        $vars.VILOCITY_WAVE_WIDE = "1"
        $vars.VILOCITY_WAVE_WIDE_ONLY = "6f225b641c5b0708,5dc3b25986622949,926c46bab3b86bd6,369cbe906ea72764,2cc4c197ee29e291"
    }
    foreach ($pair in ($ui.Extra.Text -split ';')) { $k, $v = $pair -split '=', 2; if ($k -and $k.Trim() -match '^VILOCITY_[A-Z0-9_]+$' -and $null -ne $v) { $vars[$k.Trim()] = $v.Trim() } }
    $sets = ($vars.GetEnumerator() | ForEach-Object { "`$env:$($_.Key) = '$($_.Value -replace "'", "''")'; " }) -join ''
    $switches = "-NoMonitor" + $(if ((Mode) -eq 1) { " -SeriesX" }) + $(if (-not $ui.Sandbox.IsChecked) { " -NoSandbox" })
    $play = Join-Path $PSScriptRoot "play-gta6.ps1"
    $cmd = "$sets & '$($play -replace "'", "''")' -Game '$($ui.Game.Text.Trim().Trim('"') -replace "'", "''")' $switches"
    Start-Process powershell -WindowStyle Hidden -ArgumentList "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "`"$($cmd -replace '"', '\"')`"" | Out-Null
    $state.started = Get-Date; $state.seen = $false; $ui.Play.Content = "STOP"; $ui.Status.Text = "Starting...$note"
})

# The game's state, once a second: translating shaders, running (with the frame rate of its window's title), closed.
$timer = New-Object Windows.Threading.DispatcherTimer; $timer.Interval = [TimeSpan]::FromSeconds(1)
$timer.Add_Tick({
    if (-not $state.started) { return }
    $g = Game-Process
    if ($g) {
        if ($ui.Leave.IsChecked -and -not $state.seen) { $w.Close(); return }
        $state.seen = $true
        $ui.Status.Text = if ($g.MainWindowTitle -match '([\d.]+) fps ([\d.]+) ms/frame') { "Running: {0} fps ({1} ms a frame). F8 hides the panel, Esc frees the mouse." -f $Matches[1], $Matches[2] } else { "Running." }
    } elseif (Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" | Where-Object CommandLine -like '*native shaders*') {
        $ui.Status.Text = "Translating the game's shaders (once per build, about a minute)..."
    } elseif ($state.seen -or ((Get-Date) - $state.started).TotalSeconds -gt 45) {
        $ui.Status.Text = if ($state.seen) { "The game closed." } else { "The game did not start: see Logs." }
        $state.started = $null; $ui.Play.Content = "PLAY"; & $refresh
    }
})
$timer.Start()
$w.Add_Closing({ if (-not $Shot) { Save-Choices } })
& $refresh

if ($Shot) {
    # Draws the window as it is to a picture (for looking at the layout), then leaves without saving.
    $w.Add_ContentRendered({
        $bmp = New-Object Windows.Media.Imaging.RenderTargetBitmap ([int]$w.ActualWidth), ([int]$w.ActualHeight), 96, 96, ([Windows.Media.PixelFormats]::Pbgra32)
        $bmp.Render($w); $enc = New-Object Windows.Media.Imaging.PngBitmapEncoder; $enc.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($bmp))
        $s = [IO.File]::Create($Shot); $enc.Save($s); $s.Close(); $timer.Stop(); $w.Close()
    })
}
[void]$w.ShowDialog()

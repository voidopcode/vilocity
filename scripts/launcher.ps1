# The Vilocity launcher: one window to check the PC, pick the game folder, set every option,
# change the keys and play GTA VI. Start it with the "Vilocity" shortcut in the repository's
# folder (target\release\vilocity-launcher.exe, built with the engine, makes it the first
# time). It runs scripts\play-gta6.ps1, which holds the engine's settings. What is chosen
# here is kept in %LOCALAPPDATA%\vilocity\launcher.json; what the game's own F1 menu has
# too is kept in data\menu.txt, which both read and write.
#   scripts\launcher.ps1 [-Shot <file.png> [-Page <0-5>] [-Find <word>]]   draws the window, at that page (with that word in the settings' box), to a picture and leaves
#   scripts\launcher.ps1 -Check                              tries the launcher's own logic in a folder of its own; says what failed
param([string]$Shot, [int]$Page = 0, [string]$Find, [switch]$Check)
$since = [Diagnostics.Stopwatch]::StartNew()
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Windows.Forms

# A taskbar button of its own, with the window's logo: without an app id of its own the window,
# which powershell.exe opens, is put under PowerShell's button and shows PowerShell's icon.
# (Before any window: Windows reads it when the first one opens.) And the two calls that
# bring a window to the front.
Add-Type -Namespace Vilocity -Name Taskbar -MemberDefinition @'
[DllImport("shell32.dll")] public static extern int SetCurrentProcessExplicitAppUserModelID([MarshalAs(UnmanagedType.LPWStr)] string id);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int how);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
'@
[void][Vilocity.Taskbar]::SetCurrentProcessExplicitAppUserModelID("Vilocity.Launcher")

# One launcher at a time: a second start shows the first (two of them could start two games).
$only = $null
if (-not $Shot -and -not $Check) {
    $fresh = $false; $only = [Threading.Mutex]::new($true, 'Vilocity.Launcher', [ref]$fresh)
    if (-not $fresh) {
        $first = Get-Process powershell -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -eq 'Vilocity' -and $_.Id -ne $PID } | Select-Object -First 1
        if ($first) { [void][Vilocity.Taskbar]::ShowWindow($first.MainWindowHandle, 9); [void][Vilocity.Taskbar]::SetForegroundWindow($first.MainWindowHandle) }
        exit
    }
}

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
    public static int Get(string exe) { int v = Value(exe, Enable); return v < 0 ? -1 : v != 0 ? 1 : 0; }
    // The setting `id` as the profile that holds `exe` has it; -1: not set.
    public static int Value(string exe, uint id) {
        IntPtr session = Open();
        try { IntPtr profile = ProfileOf(session, exe); if (profile == IntPtr.Zero) return -1;
            IntPtr s = Dword(id, 0); uint extra = 0;
            try { int st = F<GetSetting>(0xEA99498D, 0x73BF8338)(session, profile, id, s, ref extra); if (st == -160) return -1; Check(st, "reading the setting");
                return Marshal.ReadInt32(s, 4 + Name + 20 + 4100); }
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
            // The APIs: the driver refuses this one without administrator rights (-137, driver 621.40);
            // the profile then goes by the driver's own value for it (read back: not set), so that refusal is passed over.
            if (on) { s = Dword(Apis, 1); try { int st = write(session, profile, s, 0, 0); if (st != -137) Check(st, "writing the setting's APIs"); } finally { Marshal.FreeHGlobal(s); } }
            Check(F<On1>(0xFCBC7E14, 0)(session), "saving the driver's settings");
            return made + "Smooth Motion " + (on ? "on" : "off");
        } finally { Close(session); }
    }
}
'@

$root = Split-Path $PSScriptRoot; $scripts = $PSScriptRoot
$home_ = Join-Path $env:LOCALAPPDATA "vilocity"
# -Check works in a folder of its own: nothing of the user's is read or written.
if ($Check) { $home_ = Join-Path $env:TEMP "vilocity-launcher-check"; Remove-Item $home_ -Recurse -Force -ErrorAction SilentlyContinue }
New-Item -ItemType Directory -Force (Join-Path $home_ "logs"), (Join-Path $home_ "data") | Out-Null
# What went wrong in the launcher itself, kept for a report (it runs with no console).
$noteFile = Join-Path $home_ "logs\launcher.txt"
if ((Get-Item $noteFile -ErrorAction SilentlyContinue).Length -gt 1MB) { Remove-Item $noteFile -ErrorAction SilentlyContinue }
function Write-Note([string]$text) { try { Add-Content $noteFile ("{0:s} {1}" -f (Get-Date), $text) } catch {} }
# An error while the window is put together is written down and the next line runs, as without this.
$fail = New-Object Collections.ArrayList   # (-Check: what did not hold)
trap { Write-Note "start: $($_.Exception.Message) $($_.InvocationInfo.PositionMessage)"; [void]$fail.Add("an error outside a handler: $($_.Exception.Message) (line $($_.InvocationInfo.ScriptLineNumber))"); continue }

# The in-game menu's settings (F1), kept by the game in menu.txt in the data folder.
$menuFile = Join-Path $home_ "data\menu.txt"
$keysFile = Join-Path $home_ "data\keys.txt"
function Read-Menu {
    $m = @{}
    Get-Content $menuFile -ErrorAction SilentlyContinue | ForEach-Object { if ($_ -match '^\s*(\w+)\s*=\s*([\d.]+)\s*$') { $m[$Matches[1]] = [double]$Matches[2] } }
    $m
}
function Write-Menu($m) {
    if ($Shot) { return }
    New-Item -ItemType Directory -Force (Split-Path $menuFile) | Out-Null
    $lines = $m.GetEnumerator() | Sort-Object Name | ForEach-Object { "{0}={1}" -f $_.Name, ([double]$_.Value).ToString([Globalization.CultureInfo]::InvariantCulture) }
    Set-Content $menuFile ($lines -join "`n")
}
$file = Join-Path $home_ "launcher.json"
$exe = Join-Path $root "target\release\vilocity.exe"

# What the window remembers. Every value of the file is kept, also one this version does not
# know. (The game folder: the user's variable as it is now, not as this window's parent had it.)
$known = [Environment]::GetEnvironmentVariable('VILOCITY_GTA6_DIR', 'User'); if (-not $known) { $known = $env:VILOCITY_GTA6_DIR }
$set = @{ game = $known; extra = ""; presets = @{} }
if (Test-Path $file) {
    try {
        $old = Get-Content $file -Raw | ConvertFrom-Json
        foreach ($p in $old.PSObject.Properties) { if ($p.Name -ne 'presets') { $set[$p.Name] = $p.Value } }
        if ($old.presets) { foreach ($p in $old.presets.PSObject.Properties) { $h = @{}; foreach ($q in $p.Value.PSObject.Properties) { $h[$q.Name] = $q.Value }; $set.presets[$p.Name] = $h } }
    } catch { Write-Note "launcher.json could not be read ($($_.Exception.Message)): Vilocity's own settings are used" }
}
# (Settings kept by the launcher before this one: its frame rate was 0 none, 1 sixty, 2 thirty.)
if ($null -eq $set.rate -and $null -ne $set.cap) { $set.rate = @(0, 60, 30)[[math]::Min([int]$set.cap, 2)] }
# A remembered folder that is gone (the game was moved): the user's variable, if that one is there.
$there = { param($d) $d -and (Test-Path (Join-Path $d "game_scarlett_master.exe")) }
if (-not (& $there $set.game) -and (& $there $known)) { $set.game = $known }

$xamlText = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Vilocity" Width="1180" Height="790" MinWidth="1040" MinHeight="560" WindowStartupLocation="CenterScreen"
        Background="#0B0D12" Foreground="#E9ECF3" FontFamily="Segoe UI" FontSize="13" UseLayoutRounding="True">
  <Window.Resources>
    <!-- The logo's own two colours: its green into its violet. -->
    <LinearGradientBrush x:Key="Brand" StartPoint="0,0" EndPoint="1,1"><GradientStop Color="#3DDC97" Offset="0"/><GradientStop Color="#8B7CFF" Offset="1"/></LinearGradientBrush>
    <FontFamily x:Key="Display">Bahnschrift, Segoe UI</FontFamily>
    <Style x:Key="Dim" TargetType="TextBlock"><Setter Property="Foreground" Value="#8D95A8"/><Setter Property="FontSize" Value="12"/><Setter Property="TextWrapping" Value="Wrap"/></Style>
    <Style x:Key="H" TargetType="TextBlock"><Setter Property="FontFamily" Value="{StaticResource Display}"/><Setter Property="FontSize" Value="18"/><Setter Property="FontWeight" Value="SemiBold"/><Setter Property="Margin" Value="0,0,0,10"/></Style>
    <Style x:Key="Card" TargetType="Border"><Setter Property="Background" Value="#151922"/><Setter Property="CornerRadius" Value="12"/><Setter Property="Padding" Value="18,16"/><Setter Property="BorderBrush" Value="#232937"/><Setter Property="BorderThickness" Value="1"/><Setter Property="Margin" Value="0,0,0,16"/></Style>
    <Style TargetType="Button">
      <Setter Property="Foreground" Value="#E9ECF3"/><Setter Property="Background" Value="#1B202B"/><Setter Property="Padding" Value="14,8"/><Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="Button">
        <Border x:Name="b" Background="{TemplateBinding Background}" CornerRadius="9" Padding="{TemplateBinding Padding}" BorderThickness="1" BorderBrush="#2C3444">
          <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/></Border>
        <ControlTemplate.Triggers>
          <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#5A6580"/></Trigger>
          <Trigger Property="IsPressed" Value="True"><Setter TargetName="b" Property="Opacity" Value="0.8"/></Trigger>
          <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#E9ECF3"/></Trigger>
          <Trigger Property="IsEnabled" Value="False"><Setter TargetName="b" Property="Opacity" Value="0.4"/></Trigger>
        </ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
    </Style>
    <!-- A small button: a setup, a key. -->
    <Style x:Key="Chip" TargetType="Button" BasedOn="{StaticResource {x:Type Button}}"><Setter Property="Padding" Value="11,5"/><Setter Property="Margin" Value="0,0,6,6"/><Setter Property="FontSize" Value="12"/></Style>
    <!-- A page to go to, along the top. -->
    <Style x:Key="Tab" TargetType="RadioButton">
      <Setter Property="Foreground" Value="#8D95A8"/><Setter Property="FontSize" Value="14"/><Setter Property="Cursor" Value="Hand"/><Setter Property="Margin" Value="0,0,2,0"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="RadioButton">
        <Grid Background="Transparent">
          <Border x:Name="b" CornerRadius="8" Padding="14,9" BorderThickness="1" BorderBrush="Transparent"><ContentPresenter/></Border>
          <Border x:Name="line" Height="3" CornerRadius="2" VerticalAlignment="Bottom" Margin="14,0" Background="{StaticResource Brand}" Visibility="Collapsed"/>
        </Grid>
        <ControlTemplate.Triggers>
          <Trigger Property="IsMouseOver" Value="True"><Setter Property="Foreground" Value="#E9ECF3"/></Trigger>
          <Trigger Property="IsChecked" Value="True"><Setter Property="Foreground" Value="#E9ECF3"/><Setter TargetName="line" Property="Visibility" Value="Visible"/></Trigger>
          <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#E9ECF3"/></Trigger>
        </ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
    </Style>
    <!-- A switch: on, off. -->
    <Style TargetType="CheckBox">
      <Setter Property="Foreground" Value="#E9ECF3"/><Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="CheckBox">
        <Grid Background="Transparent"><Grid.ColumnDefinitions><ColumnDefinition Width="Auto"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions>
          <Border x:Name="track" Width="36" Height="20" CornerRadius="10" Background="#1B202B" BorderBrush="#2C3444" BorderThickness="1" VerticalAlignment="Center">
            <Ellipse x:Name="dot" Width="12" Height="12" Fill="#8D95A8" HorizontalAlignment="Left" Margin="3,0,0,0"/></Border>
          <ContentPresenter Grid.Column="1" VerticalAlignment="Center"/></Grid>
        <ControlTemplate.Triggers>
          <Trigger Property="IsChecked" Value="True"><Setter TargetName="track" Property="Background" Value="#3DDC97"/><Setter TargetName="track" Property="BorderBrush" Value="Transparent"/><Setter TargetName="dot" Property="Fill" Value="#0B0D12"/><Setter TargetName="dot" Property="HorizontalAlignment" Value="Right"/><Setter TargetName="dot" Property="Margin" Value="0,0,3,0"/></Trigger>
          <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="track" Property="BorderBrush" Value="#E9ECF3"/></Trigger>
          <Trigger Property="IsEnabled" Value="False"><Setter Property="Opacity" Value="0.4"/></Trigger>
        </ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
    </Style>
    <!-- Lists to choose from, dark like the rest (the system's own are white). -->
    <Style TargetType="ComboBoxItem">
      <Setter Property="Foreground" Value="#E9ECF3"/><Setter Property="Padding" Value="10,6"/><Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ComboBoxItem">
        <Border x:Name="b" Background="Transparent" Padding="{TemplateBinding Padding}" CornerRadius="6"><ContentPresenter/></Border>
        <ControlTemplate.Triggers>
          <Trigger Property="IsSelected" Value="True"><Setter TargetName="b" Property="Background" Value="#2A2752"/></Trigger>
          <Trigger Property="IsHighlighted" Value="True"><Setter TargetName="b" Property="Background" Value="#232937"/></Trigger>
        </ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
    </Style>
    <Style TargetType="ComboBox">
      <Setter Property="Foreground" Value="#E9ECF3"/><Setter Property="Height" Value="30"/><Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ComboBox">
        <Grid>
          <ToggleButton Focusable="False" ClickMode="Press" IsChecked="{Binding IsDropDownOpen, Mode=TwoWay, RelativeSource={RelativeSource TemplatedParent}}">
            <ToggleButton.Template><ControlTemplate TargetType="ToggleButton">
              <Border x:Name="b" Background="#1B202B" BorderBrush="#2C3444" BorderThickness="1" CornerRadius="8">
                <TextBlock Text="&#xE70D;" FontFamily="Segoe MDL2 Assets" FontSize="9" Foreground="#8D95A8" HorizontalAlignment="Right" VerticalAlignment="Center" Margin="0,0,10,0"/></Border>
              <ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#5A6580"/></Trigger></ControlTemplate.Triggers>
            </ControlTemplate></ToggleButton.Template>
          </ToggleButton>
          <ContentPresenter Margin="10,0,28,0" VerticalAlignment="Center" IsHitTestVisible="False" Content="{TemplateBinding SelectionBoxItem}" ContentTemplate="{TemplateBinding SelectionBoxItemTemplate}"/>
          <Popup IsOpen="{TemplateBinding IsDropDownOpen}" Placement="Bottom" AllowsTransparency="True" Focusable="False" PopupAnimation="None">
            <Border Background="#151922" BorderBrush="#2C3444" BorderThickness="1" CornerRadius="8" Padding="4" Margin="0,4,0,0" MinWidth="{Binding ActualWidth, RelativeSource={RelativeSource TemplatedParent}}">
              <ScrollViewer MaxHeight="300"><ItemsPresenter/></ScrollViewer></Border>
          </Popup>
        </Grid>
        <ControlTemplate.Triggers><Trigger Property="IsEnabled" Value="False"><Setter Property="Opacity" Value="0.4"/></Trigger></ControlTemplate.Triggers>
      </ControlTemplate></Setter.Value></Setter>
    </Style>
    <!-- Thin dark scroll bars. -->
    <Style TargetType="ScrollBar">
      <Setter Property="Width" Value="8"/><Setter Property="MinWidth" Value="8"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ScrollBar">
        <Track x:Name="PART_Track" Orientation="{TemplateBinding Orientation}">
          <Track.Thumb><Thumb><Thumb.Template><ControlTemplate TargetType="Thumb"><Border Background="#2C3444" CornerRadius="4" Margin="1"/></ControlTemplate></Thumb.Template></Thumb></Track.Thumb>
        </Track>
        <ControlTemplate.Triggers><Trigger Property="Orientation" Value="Vertical"><Setter TargetName="PART_Track" Property="IsDirectionReversed" Value="True"/></Trigger></ControlTemplate.Triggers>
      </ControlTemplate></Setter.Value></Setter>
      <Style.Triggers><Trigger Property="Orientation" Value="Horizontal"><Setter Property="Width" Value="Auto"/><Setter Property="MinWidth" Value="0"/><Setter Property="Height" Value="8"/><Setter Property="MinHeight" Value="8"/></Trigger></Style.Triggers>
    </Style>
    <Style TargetType="ToolTip"><Setter Property="Background" Value="#151922"/><Setter Property="Foreground" Value="#E9ECF3"/><Setter Property="BorderBrush" Value="#2C3444"/><Setter Property="Padding" Value="10,6"/><Setter Property="MaxWidth" Value="520"/></Style>
    <Style TargetType="TextBox">
      <Setter Property="Background" Value="#0F1218"/><Setter Property="Foreground" Value="#E9ECF3"/><Setter Property="CaretBrush" Value="#E9ECF3"/><Setter Property="Padding" Value="8,6"/><Setter Property="SelectionBrush" Value="#8B7CFF"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="TextBox">
        <Border x:Name="b" Background="{TemplateBinding Background}" BorderBrush="#2C3444" BorderThickness="1" CornerRadius="8">
          <ScrollViewer x:Name="PART_ContentHost"/></Border>
        <ControlTemplate.Triggers><Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#8B7CFF"/></Trigger></ControlTemplate.Triggers>
      </ControlTemplate></Setter.Value></Setter>
    </Style>
  </Window.Resources>
  <Grid Background="#0B0D12">
    <Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
    <!-- Along the top: the name, where to go, the version. -->
    <Border Background="#10131A" BorderBrush="#1C212C" BorderThickness="0,0,0,1" Padding="22,8">
      <DockPanel>
        <StackPanel DockPanel.Dock="Left" Orientation="Horizontal" Margin="0,0,30,0">
          <Image x:Name="Logo" Width="30" Height="30" Margin="0,0,10,0" RenderOptions.BitmapScalingMode="HighQuality"/>
          <TextBlock Text="Vilocity" FontFamily="{StaticResource Display}" FontSize="22" FontWeight="SemiBold" VerticalAlignment="Center"/>
        </StackPanel>
        <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Center">
          <TextBlock x:Name="Version" Foreground="#8D95A8" FontSize="12" VerticalAlignment="Center" Margin="0,0,12,0"/>
          <Button x:Name="Discord" Padding="12,6" Margin="0,0,8,0" Content="Help on Discord" ToolTip="Opens the Vilocity Discord server, where questions are answered (discord.gg/mWzP7ejTek)"/>
          <Button x:Name="UpdateRail" Padding="12,6"><TextBlock x:Name="UpdateText" Text="Update"/></Button>
        </StackPanel>
        <StackPanel Orientation="Horizontal" VerticalAlignment="Center">
          <RadioButton x:Name="Nav0" GroupName="nav" Style="{StaticResource Tab}" Content="Play" IsChecked="True"/>
          <RadioButton x:Name="Nav1" GroupName="nav" Style="{StaticResource Tab}" Content="Settings"/>
          <RadioButton x:Name="Nav2" GroupName="nav" Style="{StaticResource Tab}" Content="Keys and mouse"/>
          <RadioButton x:Name="Nav3" GroupName="nav" Style="{StaticResource Tab}" Content="This PC"/>
          <RadioButton x:Name="Nav4" GroupName="nav" Style="{StaticResource Tab}" Content="Tools"/>
          <RadioButton x:Name="Nav5" GroupName="nav" Style="{StaticResource Tab}" Content="Log"/>
        </StackPanel>
      </DockPanel>
    </Border>

    <!-- Play: the game, whether this PC takes the settings chosen, whole setups in one click. -->
    <ScrollViewer x:Name="Page0" Grid.Row="1" VerticalScrollBarVisibility="Auto" Padding="24,20,24,4">
      <Grid>
        <Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="380"/></Grid.ColumnDefinitions>
        <StackPanel Margin="0,0,16,0">
          <!-- The game's own picture (a file of your copy), and under it its name and its folder. -->
          <Border Style="{StaticResource Card}" Padding="0"><StackPanel>
            <Border CornerRadius="11,11,0,0" Height="232">
              <Border.Background><LinearGradientBrush StartPoint="0,0" EndPoint="1,1"><GradientStop Color="#12352B" Offset="0"/><GradientStop Color="#1B1C3F" Offset="0.6"/><GradientStop Color="#2B2466" Offset="1"/></LinearGradientBrush></Border.Background>
              <Border CornerRadius="11,11,0,0"><Border.Background><ImageBrush x:Name="Hero" Stretch="UniformToFill"/></Border.Background></Border>
            </Border>
            <StackPanel Margin="18,14,18,16">
              <TextBlock x:Name="GameName" FontFamily="{StaticResource Display}" FontSize="30" FontWeight="SemiBold" TextWrapping="Wrap" Text="Choose your game folder"/>
              <DockPanel Margin="0,10,0,0">
                <Button x:Name="Browse" DockPanel.Dock="Right" Content="Change folder" Margin="10,0,0,0"/>
                <TextBox x:Name="Game" ToolTip="The folder that holds game_scarlett_master.exe"/>
              </DockPanel>
            </StackPanel>
          </StackPanel></Border>
          <Border Style="{StaticResource Card}"><StackPanel>
            <DockPanel>
              <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Top">
                <Button x:Name="SetupCopy" Content="Copy mine" Padding="11,5" FontSize="12" ToolTip="Puts your settings on the clipboard as one line of text, to send to someone"/>
                <Button x:Name="SetupPaste" Content="Paste one" Padding="11,5" FontSize="12" Margin="6,0,0,0" ToolTip="Takes the settings from a line of text someone sent you (copy it first)"/>
              </StackPanel>
              <TextBlock Style="{StaticResource H}" Text="Setups" Margin="0,0,0,4"/>
            </DockPanel>
            <TextBlock Style="{StaticResource Dim}" Text="A setup is a whole group of settings in one click. Keep your own, or send one to a friend as a line of text."/>
            <WrapPanel x:Name="Presets" Margin="0,12,0,4"/>
            <DockPanel>
              <Button x:Name="PresetSave" DockPanel.Dock="Right" Content="Keep my settings as a setup" Margin="10,0,0,0"/>
              <Grid><TextBox x:Name="PresetName" MaxLength="24"/><TextBlock x:Name="PresetHint" Text="A name for it" IsHitTestVisible="False" Foreground="#6C7487" Margin="10,0,0,0" VerticalAlignment="Center"/></Grid>
            </DockPanel>
          </StackPanel></Border>
        </StackPanel>
        <StackPanel Grid.Column="1">
          <!-- What the settings chosen take of this PC, before PLAY is pressed. -->
          <Border Style="{StaticResource Card}"><StackPanel>
            <TextBlock Style="{StaticResource H}" Text="Will it run on this PC?" Margin="0,0,0,6"/>
            <DockPanel Margin="0,0,0,16"><Ellipse x:Name="FitDot" Width="10" Height="10" Margin="0,6,10,0" VerticalAlignment="Top" Fill="#8D95A8"/><TextBlock x:Name="FitText" FontSize="15" TextWrapping="Wrap" Text="Checking this PC..."/></DockPanel>
            <TextBlock Text="Graphics card memory"/>
            <Grid Height="10" Margin="0,7,0,6">
              <Border CornerRadius="3" Background="#1B202B"/>
              <Grid x:Name="CardBar"><Grid.ColumnDefinitions><ColumnDefinition Width="0*"/><ColumnDefinition Width="0*"/><ColumnDefinition Width="1*"/></Grid.ColumnDefinitions>
                <Border Background="#4A5368" CornerRadius="3,0,0,3"/><Border x:Name="CardNeed" Grid.Column="1" Background="#3DDC97"/></Grid>
            </Grid>
            <TextBlock x:Name="CardText" Style="{StaticResource Dim}" Margin="0,0,0,16"/>
            <TextBlock Text="Memory, with the page file"/>
            <Grid Height="10" Margin="0,7,0,6">
              <Border CornerRadius="3" Background="#1B202B"/>
              <Grid x:Name="RamBar"><Grid.ColumnDefinitions><ColumnDefinition Width="0*"/><ColumnDefinition Width="0*"/><ColumnDefinition Width="1*"/></Grid.ColumnDefinitions>
                <Border Background="#4A5368" CornerRadius="3,0,0,3"/><Border x:Name="RamNeed" Grid.Column="1" Background="#3DDC97"/></Grid>
            </Grid>
            <TextBlock x:Name="RamText" Style="{StaticResource Dim}" Margin="0,0,0,14"/>
            <StackPanel Orientation="Horizontal" Margin="0,0,0,14">
              <Border Width="9" Height="9" CornerRadius="2" Background="#4A5368" VerticalAlignment="Center"/><TextBlock Style="{StaticResource Dim}" Text="other programs" Margin="6,0,14,0"/>
              <Border Width="9" Height="9" CornerRadius="2" Background="#3DDC97" VerticalAlignment="Center"/><TextBlock Style="{StaticResource Dim}" Text="the game" Margin="6,0,14,0"/>
              <Border Width="9" Height="9" CornerRadius="2" Background="#1B202B" BorderBrush="#2C3444" BorderThickness="1" VerticalAlignment="Center"/><TextBlock Style="{StaticResource Dim}" Text="left over" Margin="6,0,0,0"/>
            </StackPanel>
            <!-- What this PC does best with, from its card and its memory: one press sets it. -->
            <Border Background="#10131A" CornerRadius="9" Padding="12,10" Margin="0,0,0,14"><StackPanel>
              <TextBlock Text="Best for this PC"/>
              <TextBlock x:Name="BestText" Style="{StaticResource Dim}" Margin="0,3,0,10" Text="Checking this PC..."/>
              <Button x:Name="BestUse" Content="Use these settings" HorizontalAlignment="Left"/>
            </StackPanel></Border>
            <Button x:Name="AllChecks" Content="See every check" HorizontalAlignment="Left"/>
          </StackPanel></Border>
          <Border Style="{StaticResource Card}"><StackPanel>
            <TextBlock Style="{StaticResource H}" Text="How to play" Margin="0,0,0,6"/>
            <TextBlock Style="{StaticResource Dim}" Text="On foot: W A S D move, the mouse looks, Space jumps, Shift runs, Ctrl crouches, F gets in and out. In a car the driving keys hold by themselves (W accelerates, S brakes, Space is the handbrake); G switches them by hand. F1: the game's settings. F8: the frame rate panel. F11: full screen. Esc: pause. Every key can be changed on the Keys and mouse page, and the mouse's speed is there too."/>
          </StackPanel></Border>
        </StackPanel>
      </Grid>
    </ScrollViewer>

    <!-- Settings: every option, found by a word, explained where the mouse points. -->
    <DockPanel x:Name="Page1" Grid.Row="1" Visibility="Collapsed" Margin="24,20,24,0">
      <DockPanel DockPanel.Dock="Top" Margin="0,0,0,14">
        <Button x:Name="Defaults" DockPanel.Dock="Right" Content="Back to Vilocity's own" Margin="10,0,0,0" ToolTip="Puts every setting back as Vilocity comes. Your setups are kept."/>
        <Grid><TextBox x:Name="Search"/><TextBlock x:Name="SearchHint" Text="Find a setting: dlss, mouse, fog, panel..." IsHitTestVisible="False" Foreground="#6C7487" Margin="10,0,0,0" VerticalAlignment="Center"/></Grid>
      </DockPanel>
      <Border DockPanel.Dock="Bottom" Background="#10131A" BorderBrush="#1C212C" BorderThickness="1,1,1,0" CornerRadius="10,10,0,0" Padding="16,10" MinHeight="58">
        <TextBlock x:Name="Help" Style="{StaticResource Dim}" FontSize="12.5" Text="Point at a setting to read what it does."/>
      </Border>
      <ScrollViewer VerticalScrollBarVisibility="Auto"><StackPanel>
        <Border x:Name="Notice" Style="{StaticResource Card}" Visibility="Collapsed"><DockPanel>
          <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Center" Margin="14,0,0,0">
            <Button x:Name="GetDlss" Visibility="Collapsed" Content="Get DLSS (276 MB from NVIDIA)" ToolTip="Downloads NVIDIA's Streamline SDK 2.14.16 from github.com/NVIDIA-RTX/Streamline, checks it, and keeps its DLLs in %LOCALAPPDATA%\vilocity\streamline (scripts\get-dlss.ps1)"/>
            <Button x:Name="GetFsr" Visibility="Collapsed" Content="Get FSR frame generation (87 MB from AMD)" ToolTip="Downloads AMD's signed FidelityFX DLLs (SDK v2.4.0) from github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK, checks them, and keeps them in %LOCALAPPDATA%\vilocity\fidelityfx (scripts\get-fsr.ps1)"/>
          </StackPanel>
          <TextBlock x:Name="SettingsNote" TextWrapping="Wrap" VerticalAlignment="Center"/>
        </DockPanel></Border>
        <Grid><Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions>
          <StackPanel x:Name="ColA" Margin="0,0,8,0"/><StackPanel x:Name="ColB" Grid.Column="1" Margin="8,0,0,0"/>
        </Grid>
        <!-- Every setting the engine reads at its start: found with the box at the top, set here, no file to edit. -->
        <Border x:Name="ExtraCard" Style="{StaticResource Card}"><StackPanel>
          <TextBlock Style="{StaticResource H}" Text="Every engine setting" Margin="0,0,0,6"/>
          <TextBlock x:Name="EngineNote" Style="{StaticResource Dim}" Margin="0,0,0,12"/>
          <StackPanel x:Name="EngineRows"/>
        </StackPanel></Border>
        <TextBlock x:Name="NoMatch" Style="{StaticResource Dim}" FontSize="14" Visibility="Collapsed" Margin="0,10,0,0" Text="No setting has that word. Try another, or clear the box to see them all."/>
      </StackPanel></ScrollViewer>
    </DockPanel>

    <!-- Keys: which key does what, changed by pressing it. -->
    <DockPanel x:Name="Page2" Grid.Row="1" Visibility="Collapsed" Margin="24,20,24,0">
      <DockPanel DockPanel.Dock="Top" Margin="0,0,0,14">
        <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Top" Margin="14,0,0,0">
          <Button x:Name="KeysFile" Content="Open the file"/>
          <Button x:Name="KeysReset" Content="Back to Vilocity's own" Margin="8,0,0,0"/>
        </StackPanel>
        <TextBlock Style="{StaticResource Dim}" FontSize="13" VerticalAlignment="Center" Text="Press + beside an action, then the key or mouse button for it. Click a key to take it away. A key does one thing in a layout: it moves from where it was. The running game takes a change within a second."/>
      </DockPanel>
      <ScrollViewer VerticalScrollBarVisibility="Auto"><StackPanel>
        <Border x:Name="MouseCard" Style="{StaticResource Card}"><StackPanel>
          <TextBlock Style="{StaticResource H}" Text="Mouse"/>
          <UniformGrid x:Name="MouseRows" Columns="2" Margin="0,0,-24,0"/>
        </StackPanel></Border>
        <Grid><Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions>
          <StackPanel x:Name="KeysA" Margin="0,0,8,0"/><StackPanel x:Name="KeysB" Grid.Column="1" Margin="8,0,0,0"/>
        </Grid>
      </StackPanel></ScrollViewer>
    </DockPanel>

    <!-- This PC: what it is, what it gets, what is missing. -->
    <ScrollViewer x:Name="Page3" Grid.Row="1" VerticalScrollBarVisibility="Auto" Visibility="Collapsed" Padding="24,20,24,4"><StackPanel>
      <!-- What this PC is, a tile each. -->
      <UniformGrid x:Name="Specs" Rows="1" Margin="0,0,-12,0"/>
      <Border Style="{StaticResource Card}"><StackPanel>
        <DockPanel><Button x:Name="Recheck" DockPanel.Dock="Right" Content="Check again" VerticalAlignment="Top"/><TextBlock Style="{StaticResource H}" Text="What was found"/></DockPanel>
        <StackPanel x:Name="Checks"/>
      </StackPanel></Border>
      <Border Style="{StaticResource Card}"><StackPanel><TextBlock Style="{StaticResource H}" Text="What the engine sets for this PC"/><TextBlock x:Name="Profile" TextWrapping="Wrap"/></StackPanel></Border>
    </StackPanel></ScrollViewer>


    <!-- Tools: a card each, with what it holds now. -->
    <ScrollViewer x:Name="Page4" Grid.Row="1" VerticalScrollBarVisibility="Auto" Visibility="Collapsed" Padding="24,20,8,4">
      <WrapPanel x:Name="Tools"/>
    </ScrollViewer>

    <!-- Log: what the engine wrote in its newest run. -->
    <DockPanel x:Name="Page5" Grid.Row="1" Visibility="Collapsed" Margin="24,20,24,16">
      <DockPanel DockPanel.Dock="Top" Margin="0,0,0,10">
        <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" Margin="14,0,0,0">
          <CheckBox x:Name="LogBad" VerticalAlignment="Center"><TextBlock Text="Only warnings and errors" Margin="8,0,14,0"/></CheckBox>
          <Button x:Name="LogAgain" Content="Read again"/>
          <Button x:Name="LogCopy" Content="Copy what is shown" Margin="8,0,0,0"/>
        </StackPanel>
        <Grid><TextBox x:Name="LogFind"/><TextBlock x:Name="LogFindHint" Text="Find in the log: error, dlss, shader..." IsHitTestVisible="False" Foreground="#6C7487" Margin="10,0,0,0" VerticalAlignment="Center"/></Grid>
      </DockPanel>
      <TextBlock x:Name="LogName" DockPanel.Dock="Top" Style="{StaticResource Dim}" Margin="0,0,0,8"/>
      <TextBox x:Name="LogBox" IsReadOnly="True" FontFamily="Consolas" FontSize="12" TextWrapping="NoWrap" VerticalScrollBarVisibility="Auto" HorizontalScrollBarVisibility="Auto" Background="#07080C"/>
    </DockPanel>

    <!-- Along the bottom, on every page: what is happening, what is chosen, PLAY. -->
    <Border Grid.Row="2" Background="#10131A" BorderBrush="#1C212C" BorderThickness="0,1,0,0" Padding="22,12">
      <DockPanel>
        <Button x:Name="Play" DockPanel.Dock="Right" Content="PLAY" FontFamily="{StaticResource Display}" FontSize="21" FontWeight="Bold" Padding="56,11" Margin="16,0,0,0" Background="{StaticResource Brand}" Foreground="#0B0D12"/>
        <StackPanel x:Name="Chips" DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Center" Margin="16,0,0,0"/>
        <StackPanel VerticalAlignment="Center">
          <DockPanel><Ellipse x:Name="StatusDot" Width="9" Height="9" Margin="0,5,10,0" VerticalAlignment="Top" Fill="#8D95A8"/><TextBlock x:Name="Status" TextWrapping="Wrap" MaxHeight="36" TextTrimming="CharacterEllipsis"/></DockPanel>
          <ProgressBar x:Name="Progress" Height="5" Margin="19,7,0,0" Maximum="1" Visibility="Collapsed" Foreground="#3DDC97" Background="#1B202B" BorderThickness="0"/>
        </StackPanel>
      </DockPanel>
    </Border>
  </Grid>
</Window>
'@
[xml]$xaml = $xamlText
$w = [Windows.Markup.XamlReader]::Load((New-Object System.Xml.XmlNodeReader $xaml))
if (-not $w) {
    if (-not $Check) { [void][Windows.MessageBox]::Show("The Vilocity launcher could not open. What went wrong is in`n$noteFile", "Vilocity", 'OK', 'Error') }
    exit 1
}
# Every named part of the window.
$ui = @{}; foreach ($m in [regex]::Matches($xamlText, 'x:Name="(\w+)"')) { $e = $w.FindName($m.Groups[1].Value); if ($e) { $ui[$m.Groups[1].Value] = $e } }
# Never taller or wider than the screen has room for (a 1080p screen at 150% has 720 rows).
$area = [Windows.SystemParameters]::WorkArea
$w.Width = [math]::Min($w.Width, $area.Width); $w.Height = [math]::Min($w.Height, $area.Height)
# -Shot and -Check never take the keyboard from a game that runs beside them: their window is off
# the screen and is not made the active one.
if ($Shot -or $Check) { $w.ShowActivated = $false; $w.ShowInTaskbar = $false; $w.WindowStartupLocation = 'Manual'; $w.Left = -5000; $w.Top = 0 }

$state = @{ first = (-not $Shot -and -not $Check -and -not (Test-Path $file) -and -not (Test-Path $menuFile)); sought = [bool]($Shot -or $Check); ready = $false; started = $null; seen = $false; bench = $null; benchDraws = 0; errors = 0; said = @(); syncing = $false; capture = $null; pc = $null; keep = $false; names = $null }
$tone = @{ ok = "#3DDC97"; warn = "#F5B83D"; bad = "#F2686B"; idle = "#8D95A8" }
function Brush([string]$hex) { (New-Object Windows.Media.BrushConverter).ConvertFromString($hex) }
# The line at the bottom of every page (its whole text when the mouse rests on it).
function Say([string]$text, [string]$level = 'idle') { $ui.Status.Text = $text; $ui.Status.ToolTip = $text; $ui.StatusDot.Fill = Brush $tone[$level] }
# An error in a button's or a timer's code: said and written down, and the window stays.
function Report-Error($error_) {
    $state.errors++
    # (Written down once: a timer's error may come back every second.)
    if ($state.said -notcontains $error_.Message) { $state.said += $error_.Message; Write-Note "error: $($error_.ToString())" }
    Say "Something went wrong in the launcher, which keeps running: $($error_.Message) (Written to logs\launcher.txt.)" 'bad'
}
$w.Dispatcher.Add_UnhandledException({ param($from, $e) $e.Handled = $true; Report-Error $e.Exception })
# Every timer's code runs through this (the code itself is the timer's Tag): WPF never ticks a
# timer again once its code has thrown, and the game would no longer be followed.
$safeTick = { param($from) try { & $from.Tag } catch { Report-Error $_.Exception } }

# Slow work on a thread of its own, so the window never waits for it: $script runs there with
# $with, and $then gets what it gave, back on the window's thread.
$jobs = New-Object Collections.ArrayList
$jobWatch = New-Object Windows.Threading.DispatcherTimer; $jobWatch.Interval = [TimeSpan]::FromMilliseconds(150)
function Start-Work([string]$script, [object[]]$with, [scriptblock]$then) {
    $ps = [powershell]::Create(); [void]$ps.AddScript($script); foreach ($a in $with) { [void]$ps.AddArgument($a) }
    [void]$jobs.Add(@{ ps = $ps; handle = $ps.BeginInvoke(); then = $then }); $jobWatch.Start()
}
$jobWatch.Add_Tick($safeTick); $jobWatch.Tag = {
    foreach ($j in @($jobs)) {
        if (-not $j.handle.IsCompleted) { continue }
        $jobs.Remove($j); $gave = $null
        try { $gave = @($j.ps.EndInvoke($j.handle)) } catch { Write-Note "background work: $($_.Exception.Message)" } finally { $j.ps.Dispose() }
        & $j.then $gave
    }
    if (-not $jobs.Count) { $jobWatch.Stop() }
}

# The version beside the Update button (v<workspace version>), and the commit and its date of a
# copy made with git (else the engine's build date). ([char]0xB7: the file stays ASCII.)
$ver = if ((Get-Content (Join-Path $root "Cargo.toml") -Raw -ErrorAction SilentlyContinue) -match '(?m)^version = "([^"]+)"') { $Matches[1] } else { "?" }
$commit = (git -C $root log -1 --format='%h|%cd' --date=format:'%d %b %Y' 2>$null) -split '\|'
$dot = " $([char]0xB7) "
$ui.Version.Text = "v$ver" + $(if ($LASTEXITCODE -eq 0 -and $commit.Count -eq 2) { $dot + ($commit -join $dot) } elseif (Test-Path $exe) { $dot + ("built {0:dd MMM yyyy}" -f (Get-Item $exe).LastWriteTime) } else { "" })
# The logo, in the window, its title bar and its taskbar button.
$logo = Join-Path $root "assets\logo-256.png"
if (Test-Path $logo) { $picture = New-Object Windows.Media.Imaging.BitmapImage ([Uri]$logo); $ui.Logo.Source = $picture; $w.Icon = $picture }

# The log's page: the end of the newest log, read again each time the page is opened. A word, or
# "only warnings and errors", leaves the lines that have it.
$showLogLines = {
    $ui.LogFindHint.Visibility = if ($ui.LogFind.Text) { 'Collapsed' } else { 'Visible' }
    if (-not $state.logFile) { $ui.LogName.Text = "No log yet: one is written at each start of the game."; $ui.LogBox.Text = ""; return }
    $word = $ui.LogFind.Text.Trim(); $lines = @($state.log)
    if ($ui.LogBad.IsChecked) { $lines = @($lines | Where-Object { $_ -match '^\S+ +(WARN|ERROR)\b' -or $_ -match 'panicked|exception 0x' }) }
    if ($word) { $lines = @($lines | Where-Object { $_.IndexOf($word, [StringComparison]::OrdinalIgnoreCase) -ge 0 }) }
    $ui.LogName.Text = "$($state.logFile): $($lines.Count) of its last $(@($state.log).Count) lines"
    $ui.LogBox.Text = $lines -join "`r`n"; $ui.LogBox.ScrollToEnd()
}
$showLog = {
    $newest = Get-ChildItem (Join-Path $home_ "logs") -Filter "gta6-*.log" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1
    $state.logFile = if ($newest) { $newest.FullName }; $state.log = @()
    if ($newest) {
        try {
            $fs = [IO.File]::Open($newest.FullName, 'Open', 'Read', 'ReadWrite'); $from = [math]::Max(0, $fs.Length - 300000); [void]$fs.Seek($from, 'Begin')
            $text = (New-Object IO.StreamReader($fs)).ReadToEnd(); $fs.Close()
            # (Read from the middle of the file: its first line is a part of one, unless the file is short.)
            $state.log = @($text -split "`r?`n" | Select-Object -Skip ([int]($from -gt 0)) | Where-Object { $_ } | ForEach-Object { $_ -replace '^\S+T(\d\d:\d\d:\d\d)\.\d+Z\s+', '$1 ' })
        } catch {}
    }
    & $showLogLines
}
$ui.LogFind.Add_TextChanged($showLogLines); $ui.LogBad.Add_Click($showLogLines); $ui.LogAgain.Add_Click($showLog)
$ui.LogCopy.Add_Click({ if ($ui.LogBox.Text) { Set-Clipboard -Value $ui.LogBox.Text; Say "The log shown is on the clipboard: paste it to share." 'ok' } })

# --- What this PC has for the options -------------------------------------------------------
$nvidiaCard = [bool](Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | Where-Object Name -match 'NVIDIA')
# DLSS: to be chosen only with NVIDIA's Streamline SDK there (the user's own download).
$sl = if ($env:VILOCITY_STREAMLINE_DIR) { $env:VILOCITY_STREAMLINE_DIR } else { Join-Path $home_ "streamline" }
$state.dlss = (Test-Path (Join-Path $sl "sl.interposer.dll")) -or (Test-Path (Join-Path $sl "bin\x64\sl.interposer.dll"))
# Frame generation: Streamline refuses it while Windows' GPU scheduling is off, and the game then
# runs with DLSS alone. HwSchMode 2 is on, 1 off; on Windows 11 the value is often not there
# while it is on, its default (issue 2).
$hags = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' -ErrorAction SilentlyContinue).HwSchMode
$hagsOn = $hags -eq 2 -or ($null -eq $hags -and [Environment]::OSVersion.Version.Build -ge 22000)
$fsrFg = Join-Path $home_ "fidelityfx\amd_fidelityfx_framegeneration_dx12.dll"; $state.fsrfg = Test-Path $fsrFg
# Smooth Motion as the driver holds it now for vilocity.exe; no NVIDIA driver: no such option.
$smooth = @{ driver = $null }
try { $smooth.driver = [NvSmooth]::Get('vilocity.exe') -eq 1 } catch {}
# The settings the engine's F1 menu has, read from its source: one this build of the engine does
# not know yet is not shown. (No source here: all are shown.)
$knownKeys = @(); $live = Join-Path $root "native\crates\vilocity-gpu\src\live.rs"
if (Test-Path $live) { $knownKeys = @([regex]::Matches((Get-Content $live -Raw), '=> "(\w+)"') | ForEach-Object { $_.Groups[1].Value }) }

# --- The options: one entry each ------------------------------------------------------------
# store: 'menu' is menu.txt, shared with the game's F1 menu (a change shows in both, and at once
# in a running game unless `restart`); 'set' is launcher.json, read at PLAY; 'driver' is the
# NVIDIA driver's own profile. choices: (shown, value); none: a switch (in menu.txt 1 and 0, or
# `on` and the other). only: the cards it is for. needs: what must be installed. chip: shown
# beside PLAY. private: never part of a setup (a line of text from a stranger must not turn the
# sandbox off). Defaults are what a start without the file gets (scripts\play-gta6.ps1).
$tenths = @(, @('As the game asks', 0)) + @(1..10 | ForEach-Object { , @(('{0:N1}' -f ($_ / 10)), ($_ / 10)) })
$options = @(
    @{ id = 'picture'; group = 'Picture'; label = 'Picture size'; store = 'menu'; restart = $true; chip = '{0}'; default = @(0, 2, 1)[[math]::Min(2, [int]$set.mode)]
       choices = @(@('1080p', 0), @('1440p', 1), @('4K', 2))
       help = "1080p is the game's Series S picture, 1280x720 upscaled: the fastest. 1440p and 4K are its Series X picture: sharper, about a quarter slower, and they fill a 16 GB card." }
    @{ id = 'upscaler'; group = 'Picture'; label = 'Upscaler'; store = 'menu'; restart = $true; only = 'nvidia'; needs = 'dlss'; chip = '{0}'; default = $(if ($set.dlss) { 1 } else { 0 })
       choices = @(@('DLSS Off', 0), @('DLSS Quality', 1), @('DLSS Balanced', 2), @('DLSS Performance', 3), @('DLSS Ultra Performance', 4))
       help = "DLSS draws the picture from fewer pixels, in place of the game's own upscaling (AMD FSR). Quality is the sharpest, Ultra Performance the fastest. It needs NVIDIA's Streamline SDK." }
    @{ id = 'upscaler'; group = 'Picture'; label = 'Upscaler (AMD FSR)'; store = 'menu'; restart = $true; only = 'other'; chip = '{0}'; default = 0
       choices = @(@("FSR (game's own)", 0), @('FSR Quality', 1), @('FSR Balanced', 2), @('FSR Performance', 3), @('FSR Ultra Performance', 4))
       help = "The scale AMD FSR draws the picture from. The game's own is what the console build asks for; Quality is the sharpest of the others, Ultra Performance the fastest." }
    @{ id = 'dlssmodel'; group = 'Picture'; label = 'DLSS model'; store = 'menu'; restart = $true; only = 'nvidia'; needs = 'dlss'; default = 0
       choices = @(@('Default', 0), @('J', 1), @('K', 2), @('L', 3), @('M', 4))
       help = "Which of NVIDIA's DLSS models draws the picture. Default is NVIDIA's own choice for the mode." }
    @{ id = 'fsr'; group = 'Picture'; label = "AMD's PC upscaler"; store = 'set'; only = 'other'; default = $true
       help = "Uses AMD's FSR for PCs in place of the console build's own, if its DLL is installed (a little faster)." }
    @{ id = 'sharp'; group = 'Picture'; label = 'Sharpening'; store = 'set'; default = 0; choices = $tenths
       help = "How much the PC upscaler sharpens the picture, from 0.1 (a little) to 1.0 (a lot)." }
    @{ id = 'full'; group = 'Picture'; label = 'Full screen'; store = 'set'; default = $true
       help = "Starts over the whole screen. In a window the picture is shrunk to fit the desktop, which softens it. F11 switches while you play." }

    @{ id = 'framegen'; group = 'Frame rate'; label = 'Frame generation'; store = 'menu'; restart = $true; only = 'nvidia'; needs = 'fg'; chip = 'Frame generation {0}'; default = $(if ($set.framegen) { 2 } else { 0 })
       choices = @(@('Off', 0), @('2x', 2), @('3x', 3), @('4x', 4))
       help = "DLSS shows made-up frames between the real ones (RTX 40 and 50): two to four times the frame rate shown, while the controls answer at the real one. It changes at once if it was on when the game started." }
    @{ id = 'framegen'; group = 'Frame rate'; label = 'Frame generation (AMD FSR)'; store = 'menu'; restart = $true; only = 'other'; needs = 'fsrfg'; chip = 'Frame generation {0}'; default = 0
       choices = @(@('Off', 0), @('On (2x)', 2))
       help = "AMD FSR shows a made-up frame between every two: twice the frame rate shown, while the controls answer at the real one. It needs AMD's DLLs." }
    @{ id = 'nvsmooth'; group = 'Frame rate'; label = 'NVIDIA Smooth Motion'; store = 'driver'; default = $false
       help = "The driver shows a made-up picture between every two (RTX 40 and 50). It is written into the NVIDIA driver's profile for vilocity.exe when you press PLAY; nothing else of the driver is changed." }
    @{ id = 'rate'; group = 'Frame rate'; label = 'Frames a second'; store = 'set'; restart = $true; chip = '{0}'; default = 60
       choices = @(@('No limit', 0), @('60 at most', 60), @('30 at most', 30))
       help = "How many frames a second the game may make. 30 is the console's own. 60 lets the menus, the intro and the cutscenes run smoother." }
    @{ id = 'cap'; group = 'Frame rate'; label = 'Extra limit while playing'; store = 'menu'; default = 0
       choices = @(@('None', 0), @('30', 30), @('40', 40), @('45', 45), @('50', 50), @('60', 60))
       help = "A second limit, which you can change while the game runs. It holds the frame rate at a steady number below what this PC reaches." }
    @{ id = 'steady'; group = 'Frame rate'; label = 'Steadier picture'; store = 'menu'; default = [bool]$set.steady
       help = "Fewer frames in which a character is drawn torn (seen at night), for about 1.5 ms a frame." }
    @{ id = 'tornfix'; group = 'Frame rate'; label = 'Torn-frame fix'; store = 'menu'; restart = $true; default = $false
       help = "No torn frames at all, and much slower: about 58 ms a frame in place of 41." }

    @{ id = 'light'; group = 'Light and effects'; label = 'Light in the shade'; store = 'menu'; default = 1.2
       choices = @(@('Off', 0), @('0.4', 0.4), @('0.6', 0.6), @('0.8', 0.8), @('1.0', 1.0), @('1.2', 1.2), @('1.5', 1.5))
       help = "How much light reaches what the sun does not, in place of the game's traced light. Higher is brighter shade." }
    @{ id = 'shade'; group = 'Light and effects'; label = 'Shade in creases'; store = 'menu'; default = 1.0
       choices = @(@('Off', 0), @('0.5', 0.5), @('1.0', 1.0))
       help = "How dark corners and creases are drawn, on faces too." }
    @{ id = 'night'; group = 'Light and effects'; label = 'Brighter nights'; store = 'menu'; default = $(if ($null -eq $set.night -or $set.night) { 0.05 } else { 0 })
       choices = @(@('Off', 0), @('0.03', 0.03), @('0.05', 0.05), @('0.08', 0.08), @('0.12', 0.12))
       help = "A little light where no lamp reaches, so nights are not black." }
    @{ id = 'fog'; group = 'Light and effects'; label = 'Fog light limit'; store = 'menu'; default = 1.5
       choices = @(@('Game', 0), @('1.0', 1.0), @('1.5', 1.5), @('2.0', 2.0), @('3.0', 3.0))
       help = "How bright fog and haze may glow, as a multiple of the light around. Game: no limit of ours." }
    @{ id = 'reflections'; group = 'Light and effects'; label = 'Reflections'; store = 'menu'; default = $false
       help = "A stand-in for the game's traced reflections on shiny surfaces." }
    @{ id = 'haze'; group = 'Light and effects'; label = 'Heat haze'; store = 'menu'; restart = $true; on = 0; default = $true
       help = "The game's shimmer of hot air. Off leaves it out." }
    @{ id = 'blur'; group = 'Light and effects'; label = 'Motion blur'; store = 'menu'; restart = $true; default = $false
       help = "The game's blur of fast motion." }
    @{ id = 'rays'; group = 'Light and effects'; label = 'Ray tracing'; store = 'set'; restart = $true; default = $false
       help = "The game's own traced light. For AMD Radeon cards; on NVIDIA cards it runs at 3 to 5 frames a second." }

    @{ id = 'mouse'; group = 'Mouse and sound'; label = 'Mouse sensitivity'; store = 'menu'; default = 1.0
       choices = @(0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0 | ForEach-Object { , @(('{0:0.0#}' -f $_), $_) })
       help = "How fast the camera turns with the mouse." }
    @{ id = 'invert'; group = 'Mouse and sound'; label = 'Invert mouse up and down'; store = 'menu'; default = $false
       help = "On: moving the mouse up looks down." }
    @{ id = 'smooth'; group = 'Mouse and sound'; label = 'Mouse smoothing'; store = 'menu'; default = 0
       choices = @(@('Off', 0), @('Low', 0.5), @('Medium', 1.0), @('High', 2.0))
       help = "Spreads the mouse's motion over more frames: smoother, and a little later." }
    @{ id = 'mousekeep'; group = 'Mouse and sound'; label = "Keep the mouse in the game's window"; store = 'set'; restart = $true; default = $true
       help = "The mouse stays in the game's window while the game is in front; another window in front (Alt+Tab) frees it. Off: it moves freely to other windows and screens." }
    @{ id = 'volume'; group = 'Mouse and sound'; label = 'Sound volume'; store = 'menu'; default = 100
       choices = @(0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100 | ForEach-Object { , @("$_", $_) })
       help = "The game's volume, in Windows' mixer." }

    @{ id = 'hud'; group = 'Window and panel'; label = 'Frame rate panel'; store = 'set'; restart = $true; default = $true
       help = "A small panel in a corner of the game with the frame rate, the upscaler and the memory in use. F8 hides and shows it while you play." }
    @{ id = 'panel'; group = 'Window and panel'; label = 'Panel detail'; store = 'menu'; default = 2
       choices = @(@('Hidden', 0), @('Compact', 1), @('Normal', 2), @('Detailed', 3))
       help = "How much the frame rate panel shows." }
    @{ id = 'panelside'; group = 'Window and panel'; label = 'Panel side'; store = 'menu'; default = 0
       choices = @(@('Left', 0), @('Right', 1))
       help = "The corner of the game's window the panel sits in." }
    @{ id = 'leave'; group = 'Window and panel'; label = 'Close the launcher when the game starts'; store = 'set'; private = $true; default = $false
       help = "The launcher goes away once the game's window is there. Off: it waits, minimised, and comes back when the game closes." }
    @{ id = 'sandbox'; group = 'Window and panel'; label = 'Sandbox'; store = 'set'; restart = $true; private = $true; default = $true
       help = "The game runs with no network and none of your files (recommended). Turn it off only if Windows refuses the sandbox its folders." }

    @{ id = 'logwin'; group = 'For testing'; label = 'Live log window'; store = 'set'; restart = $true; private = $true; default = $true
       help = "A console that shows what the engine writes while the game runs." }
    @{ id = 'measure'; group = 'For testing'; label = 'Measure where time goes'; store = 'set'; restart = $true; private = $true; default = $false
       help = "Runs the diagnostics build (cargo build --profile diag -p vilocity --features diagnostics), whose log says where each frame's time and the memory go; a little slower. Without that build the usual one runs." }
    @{ id = 'helper'; group = 'For testing'; label = 'Helper threads above normal'; store = 'set'; restart = $true; default = $false
       help = "To try: the engine's helper threads run above normal priority, like the thread that waits for them. Not measured on this engine (VILOCITY_HELPER_PRIORITY)." }
    @{ id = 'texpass'; group = 'For testing'; label = 'Texture time in a frame'; store = 'set'; restart = $true; default = 0
       choices = @(@('No limit', 0), @('4 ms', 4), @('8 ms', 8), @('12 ms', 12), @('16 ms', 16))
       help = "To try: at most this long a frame goes to loading textures, and what is left waits for the next frame, so textures can show a little later. Not measured on this engine (VILOCITY_TEXTURE_PASS_MS)." }
)
function Nearest-Choice($choices, [double]$v) {
    $at = 0; $best = [double]::MaxValue
    for ($i = 0; $i -lt $choices.Count; $i++) { $d = [math]::Abs([double]$choices[$i][1] - $v); if ($d -lt $best) { $best = $d; $at = $i } }
    $at
}
# A switch's two values in menu.txt.
function Menu-Number($o, $v) { if ($o.choices) { [double]$v } else { $yes = if ($null -ne $o.on) { $o.on } else { 1 }; if ($v) { [double]$yes } else { [double](1 - $yes) } } }
# What an option holds now: the file's value, else its default.
function Stored-Value($o, $menu) {
    $v = switch ($o.store) { 'menu' { if ($menu.ContainsKey($o.key)) { $menu[$o.key] } } 'set' { $set[$o.id] } 'driver' { $smooth.driver } }
    if ($null -eq $v) { $v = $o.default } elseif ($o.store -eq 'menu' -and -not $o.choices) { $v = [double]$v -eq (Menu-Number $o $true) }
    if ($o.choices) { [double]$v } else { [bool]$v }
}
# The options this PC has, by name, and what each holds.
$opt = [ordered]@{}; $val = @{}; $menuNow = Read-Menu
foreach ($o in $options) {
    if (($o.only -eq 'nvidia' -and -not $nvidiaCard) -or ($o.only -eq 'other' -and $nvidiaCard)) { continue }
    if ($o.store -eq 'menu') { $o.key = $o.id; if ($knownKeys.Count -and $o.key -notin $knownKeys) { continue } }
    if ($o.store -eq 'driver' -and $null -eq $smooth.driver) { continue }
    $o.controls = @(); $opt[$o.id] = $o; $val[$o.id] = Stored-Value $o $menuNow
}

function Save-Set {
    if ($Shot) { return }
    $set.game = $ui.Game.Text.Trim()
    # (Written beside the file and moved over it: a launcher ended mid-write leaves the old one whole.)
    try { [IO.File]::WriteAllText("$file.new", ($set | ConvertTo-Json -Depth 4)); Move-Item "$file.new" $file -Force } catch { Write-Note "launcher.json could not be saved: $($_.Exception.Message)" }
}
# One option changed: kept where it lives, and the window brought up to date.
function Set-Option([string]$id, $v, [switch]$Quiet) {
    $o = $opt[$id]; if (-not $o) { return }
    $val[$id] = $v
    if ($o.store -eq 'menu') { $m = Read-Menu; $m[$o.key] = Menu-Number $o $v; Write-Menu $m; $state.menuSeen = (Get-Item $menuFile -ErrorAction SilentlyContinue).LastWriteTimeUtc }
    elseif ($o.store -eq 'set') { $set[$id] = $v; Save-Set }
    if ($id -eq 'picture') { Show-Fit }
    Show-Summary
    if (-not $Quiet) { Say $(if ($o.restart -and $state.started) { "Saved. $($o.label) changes when the game is started again." } else { "Saved." }) 'ok' }
}
function Show-Control([string]$id) {
    $o = $opt[$id]; if (-not $o -or -not $o.control) { return }
    $state.syncing = $true
    try { foreach ($c in $o.controls) { if ($o.choices) { $c.SelectedIndex = Nearest-Choice $o.choices ([double]$val[$id]) } else { $c.IsChecked = [bool]$val[$id] } } } finally { $state.syncing = $false }
}
# What is chosen, in short, beside PLAY.
function Show-Summary {
    $ui.Chips.Children.Clear()
    foreach ($o in $opt.Values) {
        if (-not $o.chip -or ($o.control -and -not $o.control.IsEnabled)) { continue }
        $b = New-Object Windows.Controls.Border; $b.Background = Brush "#1B202B"; $b.CornerRadius = 9; $b.Padding = "10,4"; $b.Margin = "0,0,6,0"
        $t = New-Object Windows.Controls.TextBlock; $t.FontSize = 12; $t.Foreground = Brush "#B9C0D0"; $t.Text = $o.chip -f $o.choices[(Nearest-Choice $o.choices ([double]$val[$o.id]))][0]
        $b.Child = $t; [void]$ui.Chips.Children.Add($b)
    }
}
function New-Card([string]$title) {
    $b = New-Object Windows.Controls.Border; $b.Style = $w.FindResource('Card')
    $s = New-Object Windows.Controls.StackPanel; $b.Child = $s
    $t = New-Object Windows.Controls.TextBlock; $t.Style = $w.FindResource('H'); $t.Text = $title; [void]$s.Children.Add($t)
    @{ card = $b; body = $s }
}

# --- Settings page: a row for each option, a card for each group ----------------------------
$choicePicked = { param($from) if ($state.syncing -or $from.SelectedIndex -lt 0) { return }; $o = $from.Tag; Set-Option $o.id ([double]$o.choices[$from.SelectedIndex][1]); Show-Control $o.id }
$switchFlipped = { param($from) Set-Option $from.Tag.id ([bool]$from.IsChecked); Show-Control $from.Tag.id }
$rowHelp = { param($from) $o = $from.Tag
    $ui.Help.Text = "$($o.label). $($o.help)" + $(if ($o.restart) { " It changes when the game is started again." } elseif ($o.store -eq 'menu') { " It changes at once, also in a running game; the game's F1 menu has it too." } else { "" }) }
# An option's row: its name, and its list or its switch. (An option may have a row on two pages:
# each of its controls is kept, and all show what it holds.)
function New-Row($o) {
    $row = New-Object Windows.Controls.DockPanel; $row.Margin = "0,0,0,10"; $row.Background = [Windows.Media.Brushes]::Transparent; $row.Tag = $o
    if ($o.choices) { $box = New-Object Windows.Controls.ComboBox; $box.Width = 176; foreach ($ch in $o.choices) { [void]$box.Items.Add($ch[0]) }; $box.Add_SelectionChanged($choicePicked) }
    else { $box = New-Object Windows.Controls.CheckBox; $box.Add_Click($switchFlipped) }
    $box.Tag = $o; $box.VerticalAlignment = 'Center'; [Windows.Controls.DockPanel]::SetDock($box, 'Right')
    if (-not $o.control) { $o.control = $box }; $o.controls += $box
    $t = New-Object Windows.Controls.TextBlock; $t.Text = $o.label; $t.VerticalAlignment = 'Center'; $t.TextWrapping = 'Wrap'
    # (Said of the few that change in a running game; the others wait for the next start.)
    if ($o.store -eq 'menu' -and -not $o.restart) { $n = New-Object Windows.Documents.Run; $n.Text = "   live"; $n.Foreground = Brush $tone.ok; $n.FontSize = 11; [void]$t.Inlines.Add($n) }
    [void]$row.Children.Add($box); [void]$row.Children.Add($t)
    $row.Add_MouseEnter($rowHelp); $box.Add_GotFocus($rowHelp)
    $row
}
$rows = New-Object Collections.ArrayList; $cards = [ordered]@{}; $tall = @{ ColA = 0; ColB = 0 }
foreach ($o in $opt.Values) {
    if (-not $cards.Contains($o.group)) {
        $c = New-Card $o.group; $c.col = if ($tall.ColA -le $tall.ColB) { 'ColA' } else { 'ColB' }
        $cards[$o.group] = $c; $tall[$c.col] += 2; [void]$ui[$c.col].Children.Add($c.card)
    }
    $c = $cards[$o.group]; $tall[$c.col]++
    $row = New-Row $o; [void]$c.body.Children.Add($row)
    [void]$rows.Add(@{ row = $row; card = $c.card; text = ("$($o.label) $($o.group) $($o.help)").ToLower() })
    Show-Control $o.id
}
# The mouse's settings again on the Keys and mouse page, where they are looked for: the same
# options, a second row each (what they do shows when the mouse rests on them).
foreach ($id in 'mouse', 'smooth', 'invert', 'mousekeep') {
    if ($opt.Contains($id)) { $row = New-Row $opt[$id]; $row.Margin = "0,0,24,10"; $row.ToolTip = $opt[$id].help; [void]$ui.MouseRows.Children.Add($row); Show-Control $id }
}
$ui.MouseCard.Visibility = if ($ui.MouseRows.Children.Count) { 'Visible' } else { 'Collapsed' }
$ui.Search.Add_TextChanged({
    $q = $ui.Search.Text.Trim().ToLower(); $ui.SearchHint.Visibility = if ($ui.Search.Text) { 'Collapsed' } else { 'Visible' }
    $shown = @{}
    foreach ($r in $rows) { $hit = -not $q -or $r.text.Contains($q); $r.row.Visibility = if ($hit) { 'Visible' } else { 'Collapsed' }; if ($hit) { $shown[$r.card] = $true } }
    foreach ($c in $cards.Values) { $c.card.Visibility = if ($shown[$c.card]) { 'Visible' } else { 'Collapsed' } }
    # (A column with nothing found gives its room to the other.)
    $left = [bool]@($cards.Values | Where-Object { $_.col -eq 'ColA' -and $shown[$_.card] }).Count; $right = [bool]@($cards.Values | Where-Object { $_.col -eq 'ColB' -and $shown[$_.card] }).Count
    $columns = $ui.ColA.Parent.ColumnDefinitions
    $columns[0].Width = New-Object Windows.GridLength ([double][int]($left -or -not $right)), 'Star'; $columns[1].Width = New-Object Windows.GridLength ([double][int]($right -or -not $left)), 'Star'
    # The same word looks through the engine's own settings, below the switches.
    $more = Show-Engine $q
    $ui.ExtraCard.Visibility = if (-not $q -or $more -or ($q.Length -ge 3 -and -not $state.names)) { 'Visible' } else { 'Collapsed' }
    $ui.NoMatch.Visibility = if ($shown.Count -or $ui.ExtraCard.Visibility -eq 'Visible') { 'Collapsed' } else { 'Visible' }
})
# What an option needs that is not installed: it cannot be chosen, and the page says how to get it.
function Show-Needs {
    foreach ($o in $opt.Values) {
        $can = if ($o.needs -eq 'dlss') { $state.dlss } elseif ($o.needs -eq 'fg') { $state.dlss -and $hagsOn } elseif ($o.needs -eq 'fsrfg') { $state.fsrfg } else { $true }
        foreach ($c in $o.controls) { $c.IsEnabled = [bool]$can }
    }
    $note = if (-not $nvidiaCard -and -not $state.fsrfg) { "This card upscales with AMD FSR (DLSS is for NVIDIA cards). AMD's frame generation needs AMD's own DLLs, which are not part of Vilocity." }
        elseif ($nvidiaCard -and -not $state.dlss) { "DLSS and frame generation need NVIDIA's Streamline SDK, which is not part of Vilocity: it is downloaded from NVIDIA, once." }
        elseif ($nvidiaCard -and -not $hagsOn) { "Frame generation needs Windows' hardware-accelerated GPU scheduling, which is off on this PC. Turn it on in Windows Settings, System, Display, Graphics, and restart the PC." }
    $ui.SettingsNote.Text = [string]$note
    $ui.GetDlss.Visibility = if ($nvidiaCard -and -not $state.dlss) { 'Visible' } else { 'Collapsed' }
    $ui.GetFsr.Visibility = if (-not $nvidiaCard -and -not $state.fsrfg) { 'Visible' } else { 'Collapsed' }
    $ui.Notice.Visibility = if ($note) { 'Visible' } else { 'Collapsed' }
    Show-Summary
}
# Get DLSS and Get FSR: NVIDIA's Streamline SDK and AMD's DLLs fetched by scripts\get-dlss.ps1 and
# scripts\get-fsr.ps1 in a window of their own (the progress shows there); when the files are in
# place their options open up, without a restart.
$getWatch = New-Object Windows.Threading.DispatcherTimer; $getWatch.Interval = [TimeSpan]::FromSeconds(1)
$getStart = { param($from)
    $what = if ($from -eq $ui.GetDlss) { 'dlss' } else { 'fsr' }
    $get = Join-Path $scripts "get-$what.ps1"
    $state.getting = Start-Process powershell -PassThru -ArgumentList "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "& '$($get -replace "'", "''")'; `$c = `$LASTEXITCODE; Start-Sleep 6; exit `$c"
    $from.IsEnabled = $false; Say "Downloading: see its window." ; $getWatch.Start()
}
$ui.GetDlss.Add_Click($getStart); $ui.GetFsr.Add_Click($getStart)
$getWatch.Add_Tick($safeTick); $getWatch.Tag = {
    if (-not $state.getting.HasExited) { return }
    $getWatch.Stop(); $ui.GetDlss.IsEnabled = $true; $ui.GetFsr.IsEnabled = $true
    $had = $state.dlss -or $state.fsrfg
    $state.dlss = (Test-Path (Join-Path $sl "sl.interposer.dll")) -or (Test-Path (Join-Path $sl "bin\x64\sl.interposer.dll")); $state.fsrfg = Test-Path $fsrFg
    Show-Needs
    if (($state.dlss -or $state.fsrfg) -and -not $had) { Say "Downloaded: its settings can be chosen now." 'ok' } else { Say "The download did not finish: its window said why. Press the button to try again." 'warn' }
}

# --- Every engine setting: the names the engine reads at its start, set here by name --------
# They are kept as one line, NAME=VALUE; NAME=VALUE (launcher.json's "extra"), and given to the
# engine at PLAY after the options above, so what is set here wins.
function Parse-Extra([string]$text) {
    $vars = [ordered]@{}; $bad = @()
    foreach ($pair in ($text -split ';')) {
        if (-not $pair.Trim()) { continue }
        $k, $v = $pair -split '=', 2
        if ($k.Trim() -match '^VILOCITY_[A-Z0-9_]+$' -and $null -ne $v) { $vars[$k.Trim()] = $v.Trim() } else { $bad += $pair.Trim() }
    }
    @{ vars = $vars; bad = $bad }
}
function Set-Engine([string]$name, [string]$value) {
    $v = (Parse-Extra ([string]$set.extra)).vars
    if ($value.Trim()) { $v[$name] = $value.Trim() } else { $v.Remove($name) }
    $set.extra = (@($v.Keys | ForEach-Object { "$_=$($v[$_])" }) -join '; '); Save-Set
}
# Settings that have hung a whole PC in tests: said beside them.
$engineDanger = @{}
foreach ($n in 'VILOCITY_WATER_LIGHT', 'VILOCITY_WAVE_WIDE', 'VILOCITY_WAVE_WIDE_ONLY') { $engineDanger[$n] = "Careful: wide forms of the game's programs have frozen a whole PC in tests (an RTX 5080)." }
$engineSet = { param($from)
    $n = [string]$from.Tag; $v = $from.Text.Trim()
    if ($v -match '[;"]') { Say "A value cannot hold a semicolon or a double quote: $n is left as it was." 'warn'; return }
    if ([string](Parse-Extra ([string]$set.extra)).vars[$n] -eq $v) { return }
    Set-Engine $n $v
    Say $(if ($v) { "$n is $v from the game's next start." } else { "$n is the engine's own again from the game's next start." }) 'ok'
}
$engineKey = { param($from, $e) if ($e.Key -eq 'Return') { & $engineSet $from } }
# The rows. With no word in the box at the top: what you set, and what the play script sets on
# this PC. With a word: the settings whose name or note has it. Says how many it shows.
function Show-Engine([string]$q = "") {
    $ui.EngineRows.Children.Clear()
    $mine = (Parse-Extra ([string]$set.extra)).vars
    $base = [ordered]@{}; if ($state.pc -and $state.pc.engine) { foreach ($p in $state.pc.engine) { $base[[string]$p[0]] = [string]$p[1] } }
    $names = if ($q.Length -ge 3 -and $state.names) { @($state.names.Keys | Where-Object { $_.ToLower().Contains($q) -or $state.names[$_].ToLower().Contains($q) } | Sort-Object | Select-Object -First 40) }
        elseif ($q.Length -ge 3) { @() }
        else { @($mine.Keys) + @($base.Keys | Where-Object { -not $mine.Contains($_) }) }
    foreach ($n in $names) {
        $row = New-Object Windows.Controls.DockPanel; $row.Margin = "0,0,0,10"
        $box = New-Object Windows.Controls.TextBox; $box.Width = 150; $box.Tag = $n; $box.Text = [string]$mine[$n]; $box.VerticalAlignment = 'Top'
        $box.ToolTip = "Empty: the engine's own value. Enter keeps what you wrote."; $box.Add_LostFocus($engineSet); $box.Add_KeyDown($engineKey)
        [Windows.Controls.DockPanel]::SetDock($box, 'Right')
        $now = New-Object Windows.Controls.TextBlock; $now.Style = $w.FindResource('Dim'); $now.Margin = "10,8,10,0"; $now.Text = if ($base.Contains($n)) { "now $($base[$n])" } else { "" }
        [Windows.Controls.DockPanel]::SetDock($now, 'Right')
        $left = New-Object Windows.Controls.StackPanel
        $name = New-Object Windows.Controls.TextBlock; $name.Text = $n; $name.FontFamily = 'Consolas'; $name.Margin = "0,7,0,0"
        if ($mine.Contains($n)) { $name.Foreground = Brush $tone.ok }
        [void]$left.Children.Add($name)
        $note = if ($state.names) { [string]$state.names[$n] }
        if ($note) { $d = New-Object Windows.Controls.TextBlock; $d.Style = $w.FindResource('Dim'); $d.Text = $note; $d.MaxHeight = 34; $d.TextTrimming = 'CharacterEllipsis'; $d.ToolTip = $note; [void]$left.Children.Add($d) }
        if ($engineDanger[$n]) { $d = New-Object Windows.Controls.TextBlock; $d.Foreground = Brush $tone.bad; $d.FontSize = 12; $d.TextWrapping = 'Wrap'; $d.Text = $engineDanger[$n]; [void]$left.Children.Add($d) }
        [void]$row.Children.Add($box); [void]$row.Children.Add($now); [void]$row.Children.Add($left); [void]$ui.EngineRows.Children.Add($row)
    }
    $ui.EngineNote.Text = if ($q.Length -ge 3 -and -not $state.names) { "Reading the engine's settings from its source..." }
        elseif ($q.Length -ge 3) { "$($names.Count) of the engine's settings have that word$(if ($names.Count -eq 40) { ' (the first 40)' }). Write a value and press Enter; an empty box is the engine's own value. What is set here wins over the switches above, from the game's next start." }
        else { "The engine reads $(if ($state.names) { "$($state.names.Count) settings" } else { 'hundreds of settings' }) at its start; the switches above are the ones measured. To find any other, write a word of its name or of what it does in the box at the top: threads, texture, fog... Shown now: what you set (green), and what the play script sets on this PC. What is set here wins over the switches above, from the game's next start." }
    $names.Count
}
# The engine's names and the notes beside them, read from its source once, on a thread of its own.
$engineScan = @'
param($root)
$doc = @{}
Get-ChildItem (Join-Path $root "native\crates"), (Join-Path $root "apps") -Recurse -Filter *.rs -ErrorAction SilentlyContinue | ForEach-Object {
    $lines = [IO.File]::ReadAllLines($_.FullName)
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i].IndexOf("VILOCITY_") -lt 0) { continue }
        foreach ($m in [regex]::Matches($lines[$i], "VILOCITY_[A-Z0-9_]+")) {
            $n = $m.Value; if ($n.EndsWith("_")) { continue }
            $note = ""; $rank = 0
            if ($lines[$i] -match "^\s*//") {
                $last = [math]::Min($i + 2, $lines.Count - 1)
                $text = (($lines[$i..$last] | Where-Object { $_ -match "^\s*//" } | ForEach-Object { $_ -replace "^\s*//[/!]?\s?", "" }) -join " ").Replace([string][char]96, "")
                # The note from the name on. One that begins with the name says what the setting is,
                # and is kept over one that names it in passing.
                $at = [math]::Max(0, $text.IndexOf($n)); $rank = if ($at -le 12) { 2 } else { 1 }
                $note = $text.Substring([math]::Min($text.Length, $at + $n.Length)).TrimStart(" ", ":", ",", ")", ";", ".")
            }
            if (-not $doc.ContainsKey($n) -or $doc[$n][0] -lt $rank) { $doc[$n] = @($rank, $note) }
        }
    }
}
$doc.GetEnumerator() | ForEach-Object { $_.Name + "|" + $_.Value[1] }
'@
Start-Work $engineScan @($root) { param($gave)
    $state.names = @{}; foreach ($l in $gave) { $k, $d = ([string]$l) -split '\|', 2; $state.names[$k] = [string]$d }
    [void](Show-Engine $ui.Search.Text.Trim().ToLower())
}


# --- Setups: a group of settings in one click, kept by name, sent as a line of text ---------
# (A built-in one names only what it changes; $null: every option as Vilocity comes.)
$bestName = "Best for this PC"
$builtin = [ordered]@{
    $bestName = 'best'
    "Vilocity's own" = $null
    "Fastest" = @{ picture = 0; upscaler = 3; framegen = 2; reflections = $false; steady = $false; tornfix = $false; rays = $false }
    "Sharpest" = @{ picture = 1; upscaler = 1; framegen = 0; light = 1.2; shade = 1.0 }
}
$builtinHelp = @{
    $bestName = "Chosen from this PC's card and memory: the picture it takes, DLSS where it is installed, the rest as Vilocity comes."
    "Vilocity's own" = "Every setting as Vilocity comes."
    "Fastest" = "The 1080p picture with DLSS Performance and frame generation, where this PC has them."
    "Sharpest" = "The 1440p picture with DLSS Quality: the sharpest that still plays. It fills a 16 GB card."
}
function Get-Setup([switch]$All) { $h = @{}; foreach ($o in $opt.Values) { if (($All -or -not $o.private) -and $o.store -ne 'driver') { $h[$o.id] = $val[$o.id] } }; $h }
# Takes what a setup names: only options this PC has and may choose, only values that are choices,
# never a private one. Says how many were taken.
function Apply-Setup($h) {
    $n = 0
    foreach ($id in @($h.Keys)) {
        $o = $opt[[string]$id]; $v = $h[$id]
        if (-not $o -or $o.private -or $o.store -eq 'driver' -or -not $o.control.IsEnabled) { continue }
        if ($o.choices) {
            if ($null -eq $v -or $v -is [bool] -or $v -isnot [ValueType]) { continue }
            $at = Nearest-Choice $o.choices ([double]$v); if ([math]::Abs([double]$o.choices[$at][1] - [double]$v) -gt 1e-4) { continue }
            $v = [double]$o.choices[$at][1]
        } elseif ($v -isnot [bool]) { continue }
        Set-Option $o.id $v -Quiet; Show-Control $o.id; $n++
    }
    $n
}
# Every option as Vilocity comes: forgotten in both files, so the engine's own values count again.
function Reset-Options {
    $m = Read-Menu
    foreach ($o in $opt.Values) {
        if ($o.store -eq 'menu') { $m.Remove($o.key) } elseif ($o.store -eq 'set') { $set.Remove($o.id) } else { continue }
        $val[$o.id] = Stored-Value $o @{}; Show-Control $o.id
    }
    Write-Menu $m; $state.menuSeen = (Get-Item $menuFile -ErrorAction SilentlyContinue).LastWriteTimeUtc
    Save-Set; Show-Fit; Show-Summary
}
function Encode-Setup($h) { "VIL1:" + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes(($h | ConvertTo-Json -Compress))) }
function Decode-Setup([string]$text) {
    if ($text.Length -gt 6000 -or $text -notmatch 'VIL1:([A-Za-z0-9+/=]+)') { return $null }
    try { $o = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($Matches[1])) | ConvertFrom-Json; $h = @{}; foreach ($p in $o.PSObject.Properties) { $h[$p.Name] = $p.Value }; $h } catch { $null }
}
# What this PC does best with, from what was measured (Test-Fit and scripts\play-gta6.ps1): the
# 1440p picture only with a 14 GB card and 40 GB of memory, and while other programs hold no more
# than 3 GB of the card (with about 2 GB held it ran at 25 fps on an RTX 5080, with 5 to 6 GB held
# at 21 to 23); DLSS Quality where it is installed; the rest as Vilocity comes. `why` says it in words.
function Best-Setup {
    $pc = $state.pc; $h = @{ picture = 0 }; $why = @()
    $roomy = $pc -and $pc.cardTotal -ge 14 -and $pc.ram -ge 40
    if ($roomy -and ($null -eq $pc.cardFree -or $pc.cardTotal - $pc.cardFree -le 3)) { $h.picture = 1; $why += "the 1440p picture, which this card and memory take" }
    elseif ($roomy) { $why += ("the 1080p picture for now: other programs hold {0:N1} GB of the card, and 1440p wants nearly all of it (close them for 1440p)" -f ($pc.cardTotal - $pc.cardFree)) }
    else { $why += "the 1080p picture (1440p and 4K need a 16 GB card and 40 GB of memory)" }
    if ($nvidiaCard -and $opt.Contains('upscaler')) { if ($state.dlss) { $h.upscaler = 1; $why += "DLSS Quality" } else { $why += "the game's own upscaling until DLSS is installed (Settings, Get DLSS)" } }
    @{ setup = $h; why = ($why -join ', ') + ", the rest as Vilocity comes" }
}
# A setup taken (-Clean: after every option is put as Vilocity comes). The settings of before are
# remembered: Undo, beside the setups, puts them back.
function Use-Setup($h, [switch]$Clean) {
    $state.undo = Get-Setup
    if ($Clean) { Reset-Options }
    $n = if ($h) { Apply-Setup $h } else { 0 }
    Show-Presets; $n
}
$useBest = {
    if (-not $state.pc) { Say "This PC is still being checked: try again in a moment." 'warn'; return }
    $b = Best-Setup; [void](Use-Setup $b.setup -Clean)
    Say "The best settings for this PC are set: $($b.why). Undo, beside the setups, puts yours back." 'ok'
}
$ui.BestUse.Add_Click($useBest)
$presetUse = { param($from)
    $name = [string]$from.Tag
    if ($name -eq 'Undo') { $back = $state.undo; $state.undo = $null; [void](Apply-Setup $back); Show-Presets; Say "Your settings are as they were before." 'ok'; return }
    if ($name -eq $bestName) { & $useBest; return }
    if ($builtin.Contains($name) -and $null -eq $builtin[$name]) { [void](Use-Setup $null -Clean); Say "Every setting is as Vilocity comes. Undo puts yours back." 'ok'; return }
    $h = if ($builtin.Contains($name)) { $builtin[$name] } else { $set.presets[$name] }
    $n = Use-Setup $h
    Say "$name is set: $n settings$(if ($n -lt $h.Count) { ", and $($h.Count - $n) left as they were (not for this PC, or not installed)" })$(if ($state.started) { '; some change when the game is started again' }). Undo puts yours back." 'ok'
}
$presetDrop = { param($from)
    $name = [string]$from.Tag
    if ([Windows.MessageBox]::Show("Delete your setup '$name'?", "Vilocity", 'YesNo', 'Question') -ne 'Yes') { return }
    $set.presets.Remove($name); Save-Set; Show-Presets; Say "$name is deleted."
}
function Show-Presets {
    $ui.Presets.Children.Clear()
    foreach ($name in @($builtin.Keys) + @($set.presets.Keys | Sort-Object)) {
        $b = New-Object Windows.Controls.Button; $b.Style = $w.FindResource('Chip'); $b.Content = $name; $b.Tag = $name; $b.FontSize = 13; $b.Padding = "14,7"; $b.Add_Click($presetUse)
        $b.ToolTip = if ($builtin.Contains($name)) { $builtinHelp[$name] } else { "Your own setup. The small button beside it deletes it." }
        [void]$ui.Presets.Children.Add($b)
        if (-not $builtin.Contains($name)) {
            $b.Margin = "0,0,2,6"
            $x = New-Object Windows.Controls.Button; $x.Style = $w.FindResource('Chip'); $x.Content = [string][char]0xD7; $x.Tag = $name; $x.Padding = "8,7"; $x.FontSize = 13; $x.ToolTip = "Delete $name"; $x.Add_Click($presetDrop)
            [void]$ui.Presets.Children.Add($x)
        }
    }
    if ($state.undo) {
        $b = New-Object Windows.Controls.Button; $b.Style = $w.FindResource('Chip'); $b.Content = "Undo"; $b.Tag = 'Undo'; $b.FontSize = 13; $b.Padding = "14,7"; $b.Foreground = Brush $tone.ok; $b.Add_Click($presetUse)
        $b.ToolTip = "Puts your settings back as they were before the last setup"; [void]$ui.Presets.Children.Add($b)
    }
}
$ui.PresetName.Add_TextChanged({ $ui.PresetHint.Visibility = if ($ui.PresetName.Text) { 'Collapsed' } else { 'Visible' } })
$ui.PresetSave.Add_Click({
    $name = $ui.PresetName.Text.Trim()
    if (-not $name) { Say "Write a name for the setup first, in the box beside the button." 'warn'; [void]$ui.PresetName.Focus(); return }
    if ($builtin.Contains($name) -or $name -eq 'Undo') { Say "$name is one of Vilocity's own: pick another name." 'warn'; return }
    $set.presets[$name] = Get-Setup; Save-Set; Show-Presets; $ui.PresetName.Text = ""
    Say "Your settings are kept as the setup $name." 'ok'
})
$ui.SetupCopy.Add_Click({ Set-Clipboard -Value (Encode-Setup (Get-Setup)); Say "Your settings are on the clipboard as one line of text: paste it to send it." 'ok' })
$ui.SetupPaste.Add_Click({
    $h = Decode-Setup ([string](Get-Clipboard -Raw))
    if (-not $h) { Say "The clipboard holds no setup. Copy the whole line first: it starts with VIL1:" 'warn'; return }
    $n = Apply-Setup $h
    Say "The setup is taken: $n settings$(if ($n -lt $h.Count) { ", and $($h.Count - $n) left as they were (not for this PC, or not installed)" })." 'ok'
})
$ui.Defaults.Add_Click({
    if ([Windows.MessageBox]::Show("Put every setting back as Vilocity comes?`n`nYour setups, your keys and the game folder are kept.", "Vilocity", 'YesNo', 'Question') -ne 'Yes') { return }
    [void](Use-Setup $null -Clean); Say "Every setting is as Vilocity comes$(if ($state.started) { '. A running game takes them at its next start' }). Undo, on the Play page, puts yours back." 'ok'
})

# --- Keys page: data\keys.txt, a line an action, changed by pressing the key ----------------
$keyNames = @{ Return = 'Enter'; Enter = 'Enter'; Space = 'Space'; Tab = 'Tab'; Capital = 'CapsLock'; CapsLock = 'CapsLock'; Back = 'Backspace'; Insert = 'Insert'; Delete = 'Delete'; Home = 'Home'; End = 'End'
    Prior = 'PageUp'; PageUp = 'PageUp'; Next = 'PageDown'; PageDown = 'PageDown'; Pause = 'Pause'; Scroll = 'ScrollLock'; Apps = 'Apps'; Up = 'ArrowUp'; Down = 'ArrowDown'; Left = 'ArrowLeft'; Right = 'ArrowRight'
    OemComma = 'Comma'; OemPeriod = 'Period'; OemQuestion = 'Slash'; Oem2 = 'Slash'; OemSemicolon = 'Semicolon'; Oem1 = 'Semicolon'; OemQuotes = 'Quote'; Oem7 = 'Quote'; OemMinus = 'Minus'; OemPlus = 'Equals'
    OemTilde = 'Backquote'; Oem3 = 'Backquote'; OemOpenBrackets = 'LeftBracket'; Oem4 = 'LeftBracket'; OemCloseBrackets = 'RightBracket'; Oem6 = 'RightBracket'; OemPipe = 'Backslash'; Oem5 = 'Backslash'
    Add = 'NumAdd'; Subtract = 'NumSubtract'; Multiply = 'NumMultiply'; Divide = 'NumDivide'; Decimal = 'NumDecimal'
    LeftShift = 'Shift'; RightShift = 'RightShift'; LeftCtrl = 'Ctrl'; RightCtrl = 'RightCtrl'
    LeftButton = 'LeftClick'; RightButton = 'RightClick'; MiddleButton = 'MiddleClick'; XButton1 = 'Mouse4'; XButton2 = 'Mouse5' }
# The name the keys file has for a key of Windows' (none: it cannot be used. Left Alt
# and Right Alt are left out; F1, F3, F8 and F11 are Vilocity's own).
function Key-Name([string]$k) {
    if ($k -match '^[A-Z]$') { return $k }
    if ($k -match '^D(\d)$') { return $Matches[1] }
    if ($k -match '^NumPad(\d)$') { return "Num$($Matches[1])" }
    if ($k -match '^F([24-79]|10|12)$') { return $k }
    $keyNames[$k]
}
# What each action is called: GTA VI's own names, as the engine's own keys page (F1) has them, read
# from its source so that both say the same. Without the source: the pad's name for it.
$keyLabels = @{ general = @{}; walk = @{}; drive = @{}; any = @{ LOOK_UP = 'Look up (keys)'; LOOK_DOWN = 'Look down (keys)'; LOOK_LEFT = 'Look left (keys)'; LOOK_RIGHT = 'Look right (keys)' } }
$bindsFile = Join-Path $root "native\crates\vilocity-hle\src\modules\gameinput\binds.rs"
if (Test-Path $bindsFile) {
    foreach ($m in [regex]::Matches((Get-Content $bindsFile -Raw), '\("([^"]+)", "(walk|drive|general|both)", "([A-Z0-9_]+)"\)')) {
        $text = $m.Groups[1].Value -replace '^(On foot|Car): ', ''
        $keyLabels[$(if ($m.Groups[2].Value -eq 'both') { 'any' } else { $m.Groups[2].Value })][$m.Groups[3].Value] = $text.Substring(0, 1).ToUpper() + $text.Substring(1)
    }
}
function Key-Label([string]$section, [string]$name) { $l = $keyLabels[$section]; if ($l -and $l[$name]) { $l[$name] } elseif ($keyLabels.any[$name]) { $keyLabels.any[$name] } else { "$name button" } }
# The file's lines, and where each action's line is.
function Parse-Keys([string[]]$lines) {
    $section = ''; $at = New-Object Collections.ArrayList
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = ($lines[$i] -split '#')[0].Trim()
        if ($line -match '^\[(\w+)\]$') { $section = $Matches[1].ToLower() }
        elseif ($line -match '^(\w+)\s*=(.*)$') { [void]$at.Add(@{ section = $section; name = $Matches[1].ToUpper(); keys = @($Matches[2] -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ }); line = $i }) }
    }
    , $at
}
# One key more ($add) or less ($remove) for an action: the lines as they then are. A key does one
# thing in a layout, so it is taken from where it was in that layout. Of the two keys that are
# both layouts': the switch key is no action's (it takes its key from them); the get-in key is an
# action's too (F interacts and gets in, and tells Vilocity to change the layout), so it takes none.
function Edit-Keys([string[]]$lines, [string]$section, [string]$name, [string]$add, [string]$remove) {
    $all = Parse-Keys $lines; $moved = @()
    $own = $all | Where-Object { $_.section -eq $section -and $_.name -eq $name } | Select-Object -First 1
    if (-not $own) { return @{ error = "The keys file has no $name any more: open the page again." } }
    if ($add) {
        foreach ($e in $all) {
            if ($e.line -eq $own.line -or $e.keys -notcontains $add) { continue }
            if ($e.section -eq 'general') {
                if ($e.name -eq 'SWITCH' -and $section -ne 'general') { return @{ error = "$add switches between the walking and driving keys by hand. Give that another key first, or pick another key here." } }
                continue
            }
            if ($section -eq 'general') { if ($name -ne 'SWITCH') { continue } } elseif ($e.section -ne $section) { continue }
            $e.keys = @($e.keys | Where-Object { $_ -ne $add }); $e.changed = $true; $moved += Key-Label $e.section $e.name
        }
        if ($own.keys -notcontains $add) { $own.keys = @($own.keys) + $add; $own.changed = $true }
    }
    if ($remove) {
        $own.keys = @($own.keys | Where-Object { $_ -ne $remove }); $own.changed = $true
    }
    foreach ($e in $all) { if ($e.changed) { $lines[$e.line] = "{0} = {1}" -f $e.name, ($e.keys -join ', ') } }
    @{ lines = $lines; moved = $moved }
}
function Change-Keys([string]$section, [string]$name, [string]$add, [string]$remove) {
    $r = Edit-Keys @(Get-Content $keysFile) $section $name $add $remove
    if ($r.error) { Show-Keys; Say $r.error 'warn'; return }
    [IO.File]::WriteAllLines($keysFile, [string[]]$r.lines)
    Show-Keys
    $what = Key-Label $section $name
    Say $(if ($add) { "$add now does: $what$(if ($r.moved) { " (it was: $($r.moved -join ', '))" })." } else { "$remove no longer does: $what." }) 'ok'
}
$keyTake = { param($from) $k = $from.Tag; Change-Keys $k.section $k.name $null $k.key }
$keyAsk = { param($from)
    if ($state.capture) { Show-Keys; Say "Nothing changed."; return }
    $state.capture = $from.Tag; $from.Content = "press a key..."
    Say "Press the key or mouse button for: $(Key-Label $from.Tag.section $from.Tag.name). Esc: nothing changes." 'warn'
}
function Show-Keys {
    $ui.KeysA.Children.Clear(); $ui.KeysB.Children.Clear(); $state.capture = $null
    if (-not (Test-Path $keysFile)) {
        $t = New-Object Windows.Controls.TextBlock; $t.Style = $w.FindResource('Dim'); $t.FontSize = 14
        $t.Text = "The keys file is made at the game's first start. Play once, then come back here to change the keys."
        [void]$ui.KeysA.Children.Add($t); return
    }
    $all = Parse-Keys @(Get-Content $keysFile)
    foreach ($section in @($all | ForEach-Object { $_.section } | Select-Object -Unique)) {
        $title = @{ general = 'Both'; walk = 'On foot'; drive = 'In a car' }[$section]; if (-not $title) { $title = $section }
        $c = New-Card $title
        foreach ($e in @($all | Where-Object { $_.section -eq $section })) {
            $row = New-Object Windows.Controls.DockPanel; $row.Margin = "0,0,0,4"
            $t = New-Object Windows.Controls.TextBlock; $t.Text = Key-Label $section $e.name; $t.Width = 150; $t.Margin = "0,6,8,0"; $t.TextWrapping = 'Wrap'; $t.VerticalAlignment = 'Top'
            $keys = New-Object Windows.Controls.WrapPanel
            foreach ($k in $e.keys) {
                $b = New-Object Windows.Controls.Button; $b.Style = $w.FindResource('Chip'); $b.Content = $k; $b.Tag = @{ section = $section; name = $e.name; key = $k }; $b.ToolTip = "Take $k away"; $b.Add_Click($keyTake)
                [void]$keys.Children.Add($b)
            }
            $plus = New-Object Windows.Controls.Button; $plus.Style = $w.FindResource('Chip'); $plus.Content = "+"; $plus.FontSize = 15; $plus.FontWeight = 'Bold'; $plus.Padding = "10,2"; $plus.Foreground = Brush $tone.ok; $plus.Tag = @{ section = $section; name = $e.name }; $plus.ToolTip = "Add a key"; $plus.Add_Click($keyAsk)
            [void]$keys.Children.Add($plus)
            [void]$row.Children.Add($t); [void]$row.Children.Add($keys); [void]$c.body.Children.Add($row)
        }
        [void]$ui[$(if ($section -eq 'drive') { 'KeysB' } else { 'KeysA' })].Children.Add($c.card)
    }
}
# While an action waits for its key, the next key or mouse button is that key (Esc: none).
$keyGot = { param([string]$pressed)
    $c = $state.capture; $state.capture = $null
    $name = Key-Name $pressed
    if ($pressed -eq 'Escape') { Show-Keys; Say "Nothing changed." }
    elseif (-not $name) { Show-Keys; Say "$pressed cannot be used: F1, F3, F8 and F11 are Vilocity's own, and the Alt keys are left out." 'warn' }
    else { Change-Keys $c.section $c.name $name $null }
}
$w.Add_PreviewKeyDown({ param($from, $e)
    if (-not $state.capture) { return }
    $e.Handled = $true; & $keyGot ([string]$(if ($e.Key -eq 'System') { $e.SystemKey } else { $e.Key }))
})
$w.Add_PreviewMouseDown({ param($from, $e)
    if (-not $state.capture) { return }
    $e.Handled = $true; & $keyGot "$($e.ChangedButton)Button"
})
$ui.KeysFile.Add_Click({ if (Test-Path $keysFile) { Start-Process notepad $keysFile } else { Say "The keys file is made at the game's first start." 'warn' } })
$ui.KeysReset.Add_Click({
    if (-not (Test-Path $keysFile)) { Say "The keys are Vilocity's own already." ; return }
    if ([Windows.MessageBox]::Show("Put every key back as Vilocity comes?`n`nYour keys of now are kept beside the file, and the game makes its own again at its next start.", "Vilocity", 'YesNo', 'Question') -ne 'Yes') { return }
    Move-Item $keysFile (Join-Path (Split-Path $keysFile) ("keys-old-{0:yyyy-MM-dd_HH-mm-ss}.txt" -f (Get-Date))) -Force
    Show-Keys; Say "The keys are Vilocity's own again from the game's next start." 'ok'
})

# --- The pages --------------------------------------------------------------------------------
foreach ($i in 0..5) {
    $ui["Nav$i"].Tag = $i
    $ui["Nav$i"].Add_Checked({ param($from)
        foreach ($k in 0..5) { $ui["Page$k"].Visibility = if ($k -eq $from.Tag) { 'Visible' } else { 'Collapsed' } }
        if ($from.Tag -eq 2) { Show-Keys } elseif ($from.Tag -eq 4) { Show-Tools } elseif ($from.Tag -eq 5) { & $showLog }
    })
}
$ui.AllChecks.Add_Click({ $ui.Nav3.IsChecked = $true })

# --- This PC ----------------------------------------------------------------------------------
# One line of the check list: a dot (green: fine, yellow: it works but, red: it will not start), what
# was found, and for some a small button on the right that mends it.
$fixClick = { param($from) $f = $from.Tag
    if ($f.kind -eq 'folder') { & $doBrowse; return }
    if ($f.kind -eq 'build') { & $doBuild; return }
    if ($f.kind -eq 'pagefile') { Start-Process SystemPropertiesPerformance.exe; return }
    # Closed only by this button, after a question: what is not saved in that program is lost.
    if ([Windows.MessageBox]::Show("Close $($f.name) now?`n`nAnything not saved in it is lost.", "Vilocity", 'YesNo', 'Warning') -ne 'Yes') { return }
    Stop-Process -Id $f.id -Force -ErrorAction SilentlyContinue; Start-Checks
}
function Add-Check([string]$level, [string]$text, $fix = $null) {
    $row = New-Object Windows.Controls.DockPanel; $row.Margin = "0,0,0,9"
    $dot = New-Object Windows.Shapes.Ellipse; $dot.Width = 9; $dot.Height = 9; $dot.Margin = "0,5,10,0"; $dot.VerticalAlignment = "Top"; $dot.Fill = Brush $tone[$level]
    $t = New-Object Windows.Controls.TextBlock; $t.Text = $text; $t.TextWrapping = "Wrap"
    [void]$row.Children.Add($dot)
    if ($fix) {
        $b = New-Object Windows.Controls.Button; $b.Content = $fix.label; $b.Margin = "10,0,0,0"; $b.Padding = "10,2"; $b.VerticalAlignment = "Top"; $b.Tag = $fix
        [Windows.Controls.DockPanel]::SetDock($b, 'Right'); $b.Add_Click($fixClick); [void]$row.Children.Add($b)
    }
    [void]$row.Children.Add($t); [void]$ui.Checks.Children.Add($row)
}
# Looks at the PC and the folders, on a thread of its own (the window does not wait): gives the
# lines of the check list, whether the game can start, and what the card and the memory hold.
$gather = {
    param([string]$game, [bool]$big, [string]$scripts, [string]$exe)
# Whether the game will work on this PC, from what it was measured to take on 2026-11-27 (an RTX 5080 with 62 GB
# of RAM, and the logs of a second PC with a 16 GB card): at 1080p about 22 GB of RAM and 7.5 GB of the card, at
# 1440p and 4K about 27 GB and 13 GB. The card's own memory from its driver's record (the WMI figure stops at 4 GB).
function Test-Fit([double]$ram, [double]$commit = 99) {
    $card = $null; $cardGB = 0.0
    Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}' -ErrorAction SilentlyContinue | ForEach-Object {
        $p = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
        $bytes = [double]($p.'HardwareInformation.qwMemorySize')
        if ($bytes / 1GB -gt $cardGB) { $cardGB = $bytes / 1GB; $card = [string]$p.DriverDesc }
    }
    if (-not $card) { $card = (Get-CimInstance Win32_VideoController | Select-Object -First 1).Name }
    if (-not ("CpuFeature" -as [type])) { Add-Type 'using System.Runtime.InteropServices; public static class CpuFeature { [DllImport("kernel32.dll")] public static extern bool IsProcessorFeaturePresent(uint f); }' }
    $avx2 = [CpuFeature]::IsProcessorFeaturePresent(40)
    $no = @(); $limits = @()
    if (-not $avx2) { $no += "the processor has no AVX2" }
    # Shader model 6.6 and mesh shaders: RTX 20 series and newer (tested: RTX 5080); AMD RX 6000 and newer have them
    # but are hardly tested here; older cards have no mesh shaders.
    if ($card -match 'GTX|GT \d|Radeon (RX [45]\d\d0|R[579]|HD)|Intel\(R\) (UHD|HD|Iris)') { $no += "$card has no mesh shaders (an NVIDIA RTX 20 series or newer, or an AMD RX 6000 or newer, is needed)" }
    elseif ($card -notmatch 'RTX') { $limits += "$card is not an NVIDIA RTX card: it should have what the game needs, but it is hardly tested" }
    # (Advice, not a gate: only what cannot work at all stops PLAY. An 8 GB card and less RAM run, worse.)
    if ($ram -lt 24) { $limits += ("{0:N0} GB of RAM: the game takes about 22 GB at 1080p, so Windows moves part of it to the page file: expect stutters, and let Windows manage the page file's size" -f $ram) }
    elseif ($ram -lt 31) { $limits += ("{0:N0} GB of RAM: tight; close other programs" -f $ram) }
    # (An RTX 5070 PC's log: 6.8 GB of RAM free but 3.7 GB of commit, a small page file; the engine then held back textures.)
    if ($ram -ge 24 -and $commit -lt 28) { $limits += ("{0:N0} GB free with the page file: let Windows manage the page file's size" -f $commit) }
    if ($cardGB -gt 0 -and $cardGB -lt 10) { $limits += ("{0:N0} GB on the card: the game takes about 7.5 GB at 1080p besides what Windows holds, so it runs with less kept on the card and textures can load late or look wrong; close other programs that use the card" -f $cardGB) }
    if ([Environment]::OSVersion.Version.Build -lt 22000) { $limits += "Windows 10 may lack shader model 6.6" }
    $big = $cardGB -ge 14 -and $ram -ge 40
    $picture = if ($big) { "1080p for the best frame rate; 1440p works while other programs leave the card free" } else { "1080p (1440p and 4K need a 16 GB card and 40 GB of RAM)" }
    if ($no) { return @{ level = 'bad'; text = "Will it work on this PC: no. " + ($no -join "; ") + "." } }
    if ($limits) { return @{ level = 'warn'; text = "Will it work on this PC: it may, with problems. " + ($limits -join "; ") + ". Picture: $picture." } }
    $at = if ($big) { "yes" } else { "yes, at 1080p" }
    @{ level = 'ok'; text = ("Will it work on this PC: {0}. {1}, {2:N0} GB on the card, {3:N0} GB of RAM. Picture: {4}. (Measured on an RTX 5080: 24-28 fps at 1080p.)" -f $at, ($card -replace 'NVIDIA GeForce ', ''), $cardGB, $ram, $picture) }
}
    $r = @{ checks = New-Object Collections.ArrayList; ready = $true }
    function Add-Line($level, $text, $fix) { [void]$r.checks.Add(@{ level = $level; text = $text; fix = $fix }) }
    # What the play script reads of this PC and sets for it (its own first line).
    try {
        $said = @(& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $scripts "play-gta6.ps1") -Describe -Game . 2>$null)
        if ($said[0] -match '^This PC: (.+)$') { $r.profile = $Matches[1] }
        # And the engine settings it sets on this PC, a line each ("  NAME = value").
        $r.engine = @($said | ForEach-Object { if ($_ -match '^\s+(VILOCITY_[A-Z0-9_]+) = (\S+)') { , @($Matches[1], $Matches[2]) } })
    } catch {}
    if ($game -and (Test-Path (Join-Path $game "game_scarlett_master.exe"))) {
        Add-Line ok "Game folder: game_scarlett_master.exe is there"
        # The game's own name and picture, as its manifest names them (files of your copy).
        $manifest = Get-Content (Join-Path $game "MicrosoftGame.config") -Raw -ErrorAction SilentlyContinue
        if ($manifest -match 'DefaultDisplayName="([^"]+)"') { $r.name = $Matches[1] }
        if ($manifest -match 'SplashScreenImage="([^"]+)"' -and (Test-Path (Join-Path $game $Matches[1]))) { $r.splash = Join-Path $game $Matches[1] }
    }
    else { Add-Line bad "Game folder: no game_scarlett_master.exe in it. Pick the folder of your own copy of the game (Play page, Change folder)." @{ kind = 'folder'; label = 'Choose it' }; $r.ready = $false }
    # The game's shaders need shader model 6.6 of Direct3D 12: Windows 11 has it, Windows 10's
    # own may not (issue 3: the game stopped seconds in; the engine now says so and ends).
    if ([Environment]::OSVersion.Version.Build -lt 22000) { Add-Line warn "Windows 10: its own Direct3D 12 lacks shader model 6.6, so the play script fetches Microsoft's D3D12 Agility SDK once (from nuget.org, checked) and the game uses it. Windows 10 22H2 (build 19045.3393 or later) is needed." }
    if (Test-Path $exe) { Add-Line ok ("Engine: built {0:yyyy-MM-dd HH:mm}" -f (Get-Item $exe).LastWriteTime) }
    else { Add-Line bad "Engine: not built yet. In the repository's folder run: cargo build --release -p vilocity (or Tools, Build the engine)" @{ kind = 'build'; label = 'Build it' }; $r.ready = $false }
    # (As a Series S the game takes about 11 GB of the card, in the 1440p and 4K pictures about 14:
    # with 9.8 GB free in the 1440p picture its textures waited, 14,000 at a time.)
    $r.needCard = if ($big) { 14 } else { 11 }
    # The game itself, if it runs now: what it holds of the card and the memory is its own, not
    # another program's, so it counts as free for it (else a running game reads as "does not fit").
    $ours = @(Get-Process vilocity -ErrorAction SilentlyContinue)
    $gpuMem = @(Get-CimInstance Win32_PerfRawData_GPUPerformanceCounters_GPUProcessMemory -ErrorAction SilentlyContinue)
    $mine = 0.0; foreach ($m in $gpuMem) { if ($ours.Id -contains [int]($m.Name -replace '^pid_(\d+)_.*', '$1')) { $mine += $m.DedicatedUsage / 1GB } }
    if ($ours) { Add-Line ok "The game is running now: what it holds of the card and the memory is counted as its own." }
    try {
        $v = (nvidia-smi --query-gpu=name,memory.used,memory.total,driver_version --format=csv,noheader,nounits 2>$null) -split ',\s*'
        if ($v.Count -lt 4) { throw "no nvidia-smi" }
        $r.cardTotal = [double]$v[2] / 1024; $r.cardName = ($v[0] -replace 'NVIDIA GeForce ', ''); $r.driver = $v[3]
        $free = [math]::Min($r.cardTotal, ([double]$v[2] - [double]$v[1]) / 1024 + $mine); $r.cardFree = $free
        $level = if ($free -lt $r.needCard) { "warn" } else { "ok" }
        Add-Line $level ("Graphics card: {0} (driver {4}), {1:N1} of {2:N0} GB of video memory free{3}" -f ($v[0] -replace 'NVIDIA GeForce ', ''), $free, $r.cardTotal, $(if ($level -eq "warn") { ". The game wants about $($r.needCard) GB in this picture: with less, textures go missing or load late (blank eyes, wrong skin)." } else { "" }), $v[3])
        # Who else holds video memory: worth closing before playing.
        foreach ($m in ($gpuMem | Where-Object { $_.DedicatedUsage -gt 1GB })) {
            $id = [int]($m.Name -replace '^pid_(\d+)_.*', '$1'); $p = Get-Process -Id $id -ErrorAction SilentlyContinue
            if ($p -and $p.ProcessName -notin 'vilocity', 'dwm') {
                Add-Line warn ("{0} is holding {1:N1} GB of video memory. Closing it gives the game sharper textures and steadier frames." -f $p.ProcessName, ($m.DedicatedUsage / 1GB)) @{ kind = 'close'; label = 'Close it'; id = $p.Id; name = $p.ProcessName }
            }
        }
    } catch {
        # No NVIDIA tool: the card and its size from its driver's record (how much is free is not known).
        $best = Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}' -ErrorAction SilentlyContinue | ForEach-Object { Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue } |
            Where-Object { $_.'HardwareInformation.qwMemorySize' } | Sort-Object { [uint64]$_.'HardwareInformation.qwMemorySize' } -Descending | Select-Object -First 1
        if ($best) { $r.cardTotal = [uint64]$best.'HardwareInformation.qwMemorySize' / 1GB }
        $card = Get-CimInstance Win32_VideoController | Sort-Object AdapterRAM -Descending | Select-Object -First 1
        $r.cardName = [string]$card.Name
        Add-Line ok ("Graphics card: {0}" -f $card.Name)
    }
    $os = Get-CimInstance Win32_OperatingSystem
    $ram = $os.TotalVisibleMemorySize / 1MB; $commit = $os.FreeVirtualMemory / 1MB + ($ours | Measure-Object PagedMemorySize64 -Sum).Sum / 1GB
    $r.ramTotal = $os.TotalVirtualMemorySize / 1MB; $r.ramFree = $commit
    # What RAM and the page file hold together running out ends the game (out of memory); a page file Windows may grow is the cure.
    if ($commit -lt 28) { Add-Line warn ("Memory: {0:N0} GB of RAM, {1:N0} GB free with the page file. The game takes about 22 GB at 1080p, and with less free it can end out of memory: close other programs, and let Windows manage the page file (Page file, then Advanced, Virtual memory, Change, 'Automatically manage')." -f $ram, $commit) @{ kind = 'pagefile'; label = 'Page file' } }
    else { Add-Line ok ("Memory: {0:N0} GB of RAM, {1:N0} GB free with the page file" -f $ram, $commit) }
    $drive = (Get-Item $env:LOCALAPPDATA).PSDrive
    if ($drive.Free -lt 30GB) { Add-Line warn ("Disk {0}: {1:N0} GB free. The shader cache and the page file want 30 GB." -f $drive.Name, ($drive.Free / 1GB)) }
    else { Add-Line ok ("Disk {0}: {1:N0} GB free" -f $drive.Name, ($drive.Free / 1GB)) }
    $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
    Add-Line ok ("Processor: {0}, {1} cores" -f ($cpu.Name.Trim() -replace '\s+', ' '), $cpu.NumberOfCores)
    $r.ram = $ram; $r.cores = $cpu.NumberOfCores; $r.threads = $cpu.NumberOfLogicalProcessors; $r.disk = $drive.Name; $r.diskFree = $drive.Free / 1GB
    $r.cpu = (($cpu.Name -replace '\(R\)|\(TM\)|CPU|Processor|\d+-Core|@.*$', '') -replace '\s+', ' ').Trim()
    $vc = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64' -ErrorAction SilentlyContinue
    if (-not $vc -or $vc.Installed -ne 1) { Add-Line warn "Visual C++ runtime (x64): not found. The game needs it: https://aka.ms/vs/17/release/vc_redist.x64.exe" }
    $r.verdict = Test-Fit $ram $commit
    $r
}
# One of the two bars of "Will it run on this PC?": what other programs hold, what the game needs
# of the rest (green: it fits; yellow: it does not), and what is left over.
function Show-Bar([string]$name, $total, $free, [double]$need, [string]$short) {
    $cols = $ui["${name}Bar"].ColumnDefinitions; $star = { param($v) New-Object Windows.GridLength ([math]::Max(0.0, [double]$v)), 'Star' }
    if (-not $total) { $cols[0].Width = & $star 0; $cols[1].Width = & $star 0; $cols[2].Width = & $star 1; $ui["${name}Text"].Text = "The game needs about $need GB. How much this PC has could not be read."; return }
    $known = $null -ne $free; if (-not $known) { $free = $total }
    $fits = $need -le $free
    $cols[0].Width = & $star ($total - $free); $cols[1].Width = & $star ([math]::Min($need, $free)); $cols[2].Width = & $star ($free - $need)
    $ui["${name}Need"].Background = Brush $tone[$(if ($fits) { 'ok' } else { 'warn' })]
    $ui["${name}Text"].Text = if (-not $known) { "The game needs about {0} of this card's {1:N0} GB. How much of it other programs hold cannot be read on this card." -f $need, $total }
        elseif ($fits) { "The game needs about {0} GB. {1:N1} of {2:N0} GB are free." -f $need, $free, $total }
        else { "The game needs about {0} GB, and {1:N1} of {2:N0} GB are free: {3}" -f $need, $free, $total, $short }
}
function Show-Fit {
    $pc = $state.pc; if (-not $pc) { return }
    $big = $opt.Contains('picture') -and [double]$val.picture -ne 0
    # (Measured on 2026-11-27: at 1080p about 22 GB of memory, at 1440p and 4K about 27.)
    Show-Bar 'Card' $pc.cardTotal $pc.cardFree $(if ($big) { 14 } else { 11 }) "textures can go missing or load late. Close programs that use the card, or pick the 1080p picture."
    Show-Bar 'Ram' $pc.ramTotal $pc.ramFree $(if ($big) { 27 } else { 22 }) "the game can end out of memory. Close other programs, and let Windows manage the page file."
}
# This PC in four tiles: the card, the memory, the processor, the disk.
function Show-Specs($r) {
    $ui.Specs.Children.Clear()
    $card = if ($r.cardTotal) { "{0:N0} GB of video memory{1}{2}" -f $r.cardTotal, $(if ($null -ne $r.cardFree) { ", {0:N1} GB free" -f $r.cardFree }), $(if ($r.driver) { ", driver $($r.driver)" }) } else { "Its memory could not be read" }
    $tiles = @(
        @("Graphics card", [string]$r.cardName, $card),
        @("Memory", ("{0:N0} GB" -f $r.ram), ("{0:N0} GB free with the page file" -f $r.ramFree)),
        @("Processor", [string]$r.cpu, "$($r.cores) cores, $($r.threads) threads"),
        @("Disk $($r.disk):", ("{0:N0} GB free" -f $r.diskFree), "The shader cache and the page file want 30 GB")
    )
    foreach ($t in $tiles) {
        $b = New-Object Windows.Controls.Border; $b.Style = $w.FindResource('Card'); $b.Margin = "0,0,12,16"
        $s = New-Object Windows.Controls.StackPanel; $b.Child = $s
        $label = New-Object Windows.Controls.TextBlock; $label.Style = $w.FindResource('Dim'); $label.Text = $t[0]
        $value = New-Object Windows.Controls.TextBlock; $value.FontFamily = $w.FindResource('Display'); $value.FontSize = 22; $value.FontWeight = 'SemiBold'; $value.Margin = "0,4,0,4"; $value.TextTrimming = 'CharacterEllipsis'; $value.Text = $t[1]; $value.ToolTip = $t[1]
        $more = New-Object Windows.Controls.TextBlock; $more.Style = $w.FindResource('Dim'); $more.Text = $t[2]
        [void]$s.Children.Add($label); [void]$s.Children.Add($value); [void]$s.Children.Add($more); [void]$ui.Specs.Children.Add($b)
    }
}
# The game's folder, looked for on this PC's fixed drives when none is known: four folders deep,
# Windows' own folders left out, on a thread of its own.
$gameScan = @'
param([string[]]$roots)
foreach ($root in $roots) {
    if (Test-Path (Join-Path $root "game_scarlett_master.exe")) { return $root }
    foreach ($top in @(Get-ChildItem $root -Directory -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -notmatch '^(Windows|Program Files|Program Files \(x86\)|ProgramData|\$Recycle\.Bin|System Volume Information|Recovery)$' })) {
        if (Test-Path (Join-Path $top.FullName "game_scarlett_master.exe")) { return $top.FullName }
        $hit = Get-ChildItem $top.FullName -Filter game_scarlett_master.exe -File -Recurse -Depth 2 -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($hit) { return $hit.DirectoryName }
    }
}
'@
function Find-Game {
    Say "No game folder yet: looking for your game on this PC's drives..."
    $roots = @([IO.DriveInfo]::GetDrives() | Where-Object { $_.DriveType -eq 'Fixed' -and $_.IsReady } | ForEach-Object { $_.RootDirectory.FullName })
    Start-Work $gameScan (, $roots) { param($gave)
        $found = if ($gave) { [string]$gave[-1] }
        if ($found) { $ui.Game.Text = $found; Save-Set; Start-Checks -Keep; Say "Found your game in $found." 'ok' }
        else { Say "Your game was not found on this PC's drives: press Change folder and pick the folder that holds game_scarlett_master.exe." 'bad' }
    }
}
function Show-Machine($r) {
    if (-not $r -or -not $r.verdict) { Say "This PC could not be checked: see logs\launcher.txt. PLAY works all the same." 'warn'; $ui.Play.IsEnabled = $true; return }
    $ui.Checks.Children.Clear(); $ui.Profile.Text = [string]$r.profile
    $ui.GameName.Text = if ($r.name) { $r.name } elseif ($r.ready) { "Your game" } else { "Choose your game folder" }
    $ui.Hero.ImageSource = $null
    if ($r.splash) { try { $splash = New-Object Windows.Media.Imaging.BitmapImage; $splash.BeginInit(); $splash.CacheOption = 'OnLoad'; $splash.DecodePixelWidth = 1280; $splash.UriSource = [Uri]$r.splash; $splash.EndInit(); $ui.Hero.ImageSource = $splash } catch {} }
    # The answer first.
    Add-Check $r.verdict.level $r.verdict.text
    foreach ($c in $r.checks) { Add-Check $c.level $c.text $c.fix }
    $state.ready = [bool]$r.ready; $state.verdict = $r.verdict.level; $state.pc = $r
    $answer = (($r.verdict.text -replace '^Will it work on this PC: ', '') -split '\. ')[0]
    $ui.FitText.Text = $answer.Substring(0, 1).ToUpper() + $answer.Substring(1).TrimEnd('.') + "."; $ui.FitDot.Fill = Brush $tone[$r.verdict.level]
    Show-Fit; [void](Show-Engine $ui.Search.Text.Trim().ToLower()); Show-Specs $r
    $best = (Best-Setup).why; $ui.BestText.Text = $best.Substring(0, 1).ToUpper() + $best.Substring(1) + "."
    # (Advice only: PLAY stays on. Only a missing game folder or engine, which nothing could start, turns it off.)
    if (-not $state.started) {
        $ui.Play.IsEnabled = $state.ready
        if (-not $state.keep) { if (-not $state.ready) { Say "Not ready yet: see This PC." 'bad' } elseif ($state.verdict -eq 'bad') { Say "Ready, but this PC may not run the game: see This PC." 'warn' } else { Say "Ready." 'ok' } }
    }
    $state.keep = $false
    # A first start (nothing kept yet): the best settings for this PC, set by themselves.
    if ($state.first) { $state.first = $false; & $useBest; Say "First start: the best settings for this PC are set ($((Best-Setup).why)). Any of them can be changed on the Settings page." 'ok' }
    # No game folder: looked for on this PC's drives, once.
    if (-not $r.ready -and -not $state.sought -and -not (& $there $ui.Game.Text.Trim().Trim('"'))) { $state.sought = $true; Find-Game }
}
# -Keep: the line at the bottom stays as it is (it says how the game ended).
function Start-Checks([switch]$Keep) {
    if ($state.checking) { return }
    $state.checking = $true; $state.keep = [bool]$Keep; $state.checked = $ui.Game.Text
    if (-not $state.started -and -not $Keep) { Say "Checking this PC..." }
    $big = $opt.Contains('picture') -and [double]$val.picture -ne 0
    Start-Work $gather.ToString() @($ui.Game.Text.Trim().Trim('"'), $big, $scripts, $exe) { param($gave) $state.checking = $false; Show-Machine $(if ($gave) { $gave[-1] }) }
}

# The game: the vilocity.exe with a window (the play and sandbox processes have none). Get-Process, not
# WMI: this is asked once a second while the game runs, and a WMI query costs tens of milliseconds.
function Game-Process {
    Get-Process vilocity -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } | Select-Object -First 1
}
# PLAY in the logo's colours; STOP plain.
function Show-Play([bool]$running) {
    $ui.Play.Content = if ($running) { "STOP" } else { "PLAY" }
    if ($running) { $ui.Play.Background = Brush "#1B202B"; $ui.Play.Foreground = Brush "#E9ECF3" } else { $ui.Play.Background = $w.FindResource('Brand'); $ui.Play.Foreground = Brush "#0B0D12" }
}

$ui.Game.Text = [string]$set.game
$doBrowse = {
    $d = New-Object Windows.Forms.FolderBrowserDialog; $d.Description = "The folder that holds game_scarlett_master.exe"
    if ($ui.Game.Text -and (Test-Path $ui.Game.Text)) { $d.SelectedPath = $ui.Game.Text }
    if ($d.ShowDialog() -eq 'OK') { $ui.Game.Text = $d.SelectedPath; Save-Set; Start-Checks }
}
$ui.Browse.Add_Click($doBrowse)
$ui.Game.Add_LostFocus({ if ($ui.Game.Text -ne $state.checked) { Save-Set; Start-Checks } })
$ui.Recheck.Add_Click({ Start-Checks })
$doSaves = { $d = Join-Path $home_ "data"; New-Item -ItemType Directory -Force $d | Out-Null; Start-Process explorer $d }
$doLogs = { $d = Join-Path $home_ "logs"; New-Item -ItemType Directory -Force $d | Out-Null; Start-Process explorer $d }
$doIssues = { Start-Process "https://github.com/voidopcode/vilocity/issues" }
# A report to paste where help is asked: the build, this PC, the settings and the end of the newest
# log, with the user's own folder name taken out.
$doReport = {
    $newest = Get-ChildItem (Join-Path $home_ "logs") -Filter "gta6-*.log" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1
    $now = Get-Setup -All
    $lines = @("Vilocity $($ui.Version.Text)", "This PC: $($ui.Profile.Text)", [string]$(if ($state.pc) { $state.pc.verdict.text }),
        "Settings: " + (($now.Keys | Sort-Object | ForEach-Object { "$_=$($now[$_])" }) -join ', '), "Engine settings by name: $($set.extra)")
    if ($newest) { $lines += "The end of $($newest.Name):"; $lines += Get-Content $newest.FullName -Tail 60 } else { $lines += "No log yet." }
    Set-Clipboard -Value (($lines -join "`r`n").Replace($env:USERPROFILE, '%USERPROFILE%').Replace($env:USERNAME, '(user)'))
    Say "The report is on the clipboard: paste it in the Discord's help channel or a GitHub issue." 'ok'
}

# Save backups: each title's saves (data\<title>\saves) copied to backups\<date and time>; at each
# PLAY the newest 10 are kept. -NoTrim: none is let go (before a restore, which may pick the oldest).
$backups = Join-Path $home_ "backups"
function Backup-Saves([switch]$NoTrim) {
    $titles = @(Get-ChildItem (Join-Path $home_ "data") -Directory -ErrorAction SilentlyContinue | Where-Object { Test-Path (Join-Path $_.FullName "saves") })
    if (-not $titles) { return $null }
    $to = Join-Path $backups (Get-Date -Format 'yyyy-MM-dd_HH-mm-ss')
    foreach ($t in $titles) {
        New-Item -ItemType Directory -Force (Join-Path $to $t.Name) | Out-Null
        Copy-Item (Join-Path $t.FullName "saves") (Join-Path $to $t.Name) -Recurse -Force
    }
    if (-not $NoTrim) { Get-ChildItem $backups -Directory | Sort-Object Name -Descending | Select-Object -Skip 10 | Remove-Item -Recurse -Force }
    $to
}
$doBackups = { New-Item -ItemType Directory -Force $backups | Out-Null; Start-Process explorer $backups }
$doRestore = {
    if (Game-Process) { Say "Close the game first, then restore." 'warn'; return }
    $pick = [string]$toolBox.backups.SelectedItem
    if (-not $pick) { Say "There is no backup yet: one is made at each PLAY." 'warn'; return }
    $titles = @(Get-ChildItem (Join-Path $backups $pick) -Directory -ErrorAction SilentlyContinue | Where-Object { Test-Path (Join-Path $_.FullName "saves") })
    if (-not $titles) { Say "The backup $pick holds no saves." 'warn'; return }
    $sure = [Windows.MessageBox]::Show("Put back the saves of $pick?`n`nYour saves of now are copied to the backups first.", "Restore saves", 'YesNo')
    if ($sure -ne 'Yes') { return }
    $now = Backup-Saves -NoTrim
    foreach ($t in $titles) {
        $dst = Join-Path $home_ "data\$($t.Name)"
        New-Item -ItemType Directory -Force $dst | Out-Null
        Remove-Item (Join-Path $dst "saves") -Recurse -Force -ErrorAction SilentlyContinue
        Copy-Item (Join-Path $t.FullName "saves") $dst -Recurse -Force
    }
    Show-Tools; Say "Saves put back from $pick$(if ($now) { "; those of before are in $(Split-Path $now -Leaf)" })." 'ok'
}

# The benchmark: PLAY, then 60 seconds of the world measured from the window's frame rate (the
# timer), and what it found put on the clipboard to share.
$benchDone = {
    $f = @($state.bench | Sort-Object)
    $avg = ($f | Measure-Object -Average).Average
    $gpu = (Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | Sort-Object AdapterRAM -Descending | Select-Object -First 1).Name
    $cpu = (Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1).Name
    $ram = (Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue).TotalPhysicalMemory / 1GB
    $how = (@($ui.Chips.Children | ForEach-Object { $_.Child.Text }) -join ', ')
    $text = "Vilocity benchmark, {0:yyyy-MM-dd}: {1}, {2}, {3:N0} GB RAM, {4}. 60 s of play: median {5:N1} fps, average {6:N1}, 5% low {7:N1}, highest {8:N1} ({9:N0} draws a frame)." -f (Get-Date), $gpu, $cpu.Trim(), $ram, $how, $f[[int]($f.Count / 2)], $avg, $f[[int]($f.Count * 0.05)], $f[-1], ($state.benchDraws / $f.Count)
    $state.bench = $null; $set.bench = $text; Save-Set
    try { Set-Clipboard -Value $text } catch {}
    Say ($text + " (Copied: paste it to share.)") 'ok'
    [Windows.MessageBox]::Show($text + "`n`nIt is on the clipboard: paste it to share.", "Benchmark") | Out-Null
}
$doBench = {
    if ($state.started) { Say "Close the game first: the benchmark starts it." 'warn'; return }
    $state.bench = New-Object 'System.Collections.Generic.List[double]'; $state.benchDraws = 0
    $ui.Play.RaiseEvent((New-Object Windows.RoutedEventArgs ([Windows.Controls.Primitives.ButtonBase]::ClickEvent)))
}

# Update: asks GitHub first. Up to date: says so, nothing else runs. Newer: asks, then gets it
# (git pull) and builds the engine in the background, here, and opens the launcher again.
function Behind-Count { $n = git -C $root rev-list --count 'HEAD..@{u}' 2>$null; if ($LASTEXITCODE -eq 0) { [int]$n } else { -1 } }
function Show-Update([int]$n) {
    if ($n -gt 0) { $ui.UpdateText.Text = "Update ready ($n new)"; $ui.UpdateRail.Background = Brush "#5B4FD6" }
    elseif ($n -eq 0) { $ui.UpdateText.Text = "Up to date"; $ui.UpdateRail.ClearValue([Windows.Controls.Control]::BackgroundProperty) }
    if ($n -ge 0) { $toolInfo.update.Text = "$($ui.Version.Text): $($ui.UpdateText.Text.ToLower())" }
}
$doUpdate = {
    if (-not (Test-Path (Join-Path $root ".git"))) { [void][Windows.MessageBox]::Show("This copy of Vilocity was not made with git: download the newest from github.com/voidopcode/vilocity.", "Vilocity"); return }
    if ($state.updating) { return }
    git -C $root fetch --quiet 2>$null
    $n = Behind-Count; Show-Update $n
    if ($n -lt 0) { [void][Windows.MessageBox]::Show("GitHub could not be reached. Check the internet connection and try again.", "Vilocity", 'OK', 'Warning'); return }
    if ($n -eq 0) { [void][Windows.MessageBox]::Show("Vilocity is up to date.", "Vilocity", 'OK', 'Information'); return }
    if ($state.started -or (Game-Process)) { [void][Windows.MessageBox]::Show("A new version of Vilocity is ready. Close the game first, then press Update again: it builds the game's engine again.", "Vilocity", 'OK', 'Information'); return }
    $sure = [Windows.MessageBox]::Show("A new version of Vilocity is ready ($n new $(if ($n -eq 1) { 'change' } else { 'changes' })).`n`nUpdate now? It downloads it and builds the engine again: a few minutes, here in the launcher. The launcher opens again when it is done.", "Vilocity", 'YesNo', 'Question')
    if ($sure -ne 'Yes') { return }
    $log = Join-Path $home_ "logs\update.txt"; New-Item -ItemType Directory -Force (Split-Path $log) | Out-Null
    $state.updating = Start-Process cmd -ArgumentList '/c', "cd /d `"$root`" && (git pull --ff-only && cargo build --release -p vilocity) > `"$log`" 2>&1" -WindowStyle Hidden -PassThru
    $null = $state.updating.Handle   # (so that its exit code can be read once it ends)
    $ui.UpdateText.Text = "Updating..."; $ui.Play.IsEnabled = $false
    Say "Updating Vilocity: downloading, then building the engine (a few minutes)..."
    $updWatch.Start()
}
# While it updates: the build's last line as the status; at the end the launcher opens again, or the log.
$updWatch = New-Object Windows.Threading.DispatcherTimer; $updWatch.Interval = [TimeSpan]::FromSeconds(1)
$updWatch.Add_Tick($safeTick); $updWatch.Tag = {
    $log = Join-Path $home_ "logs\update.txt"
    if (-not $state.updating.HasExited) {
        $last = Get-Content $log -Tail 1 -ErrorAction SilentlyContinue
        if ($last -match '^\s*(Compiling|Updating|Fast-forward)\s*(\S*)') { Say "Updating Vilocity: $($Matches[1].ToLower()) $($Matches[2])..." }
        return
    }
    $updWatch.Stop(); $code = $state.updating.ExitCode; $state.updating = $null; $ui.Play.IsEnabled = $true
    if ($code -eq 0) {
        [void][Windows.MessageBox]::Show("Vilocity is updated. The launcher opens again now, with the new version.", "Vilocity", 'OK', 'Information')
        # (This launcher's own lock is let go first: the new one would otherwise only show this one.)
        if ($only) { $only.ReleaseMutex(); $only.Dispose(); $only = $null }
        $again = Join-Path $root "target\release\vilocity-launcher.exe"
        if (Test-Path $again) { Start-Process $again } else { Start-Process powershell -WindowStyle Hidden -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"" }
        $w.Close()
    } else {
        $ui.UpdateText.Text = "Update failed"; Say "The update did not finish: its log is open. Ask in the Discord's help channel with it if it is not clear." 'bad'
        Start-Process notepad $log
    }
}
# The Update button at the top, always there: lit up when GitHub has a newer Vilocity (one git fetch, in the background, at start).
$ui.UpdateRail.Add_Click({ & $doUpdate }); $ui.Discord.Add_Click({ & $doDiscord })
if (-not $Shot -and -not $Check -and (Test-Path (Join-Path $root ".git"))) {
    try { $state.fetch = Start-Process git -ArgumentList '-C', "`"$root`"", 'fetch', '--quiet' -WindowStyle Hidden -PassThru } catch {}
}
$updCheck = New-Object Windows.Threading.DispatcherTimer; $updCheck.Interval = [TimeSpan]::FromSeconds(1)
$updCheck.Add_Tick($safeTick); $updCheck.Tag = {
    if (-not $state.fetch.HasExited) { return }
    $updCheck.Stop(); Show-Update (Behind-Count)
}
if ($state.fetch) { $updCheck.Start() }
# The "Vilocity" shortcut in the repository's folder: opens the launcher with no console window,
# through target\release\vilocity-launcher.exe (built with the engine).
$launcherExe = Join-Path $root "target\release\vilocity-launcher.exe"
if (-not $Shot -and -not $Check -and (Test-Path $launcherExe)) {
    try {
        $lnk = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $root "Vilocity.lnk"))
        if ($lnk.TargetPath -ne $launcherExe) {
            $lnk.TargetPath = $launcherExe; $lnk.WorkingDirectory = $root; $lnk.Description = "Vilocity launcher"
            $lnk.IconLocation = (Join-Path $root "assets\logo.ico") + ",0"; $lnk.Save()
        }
    } catch {}
}
$doBuild = { Start-Process cmd -ArgumentList '/k', "cd /d `"$root`" && cargo build --release -p vilocity && echo. && echo Built. Close this window and press Check again." }

# Every pipeline the game has made in earlier runs (the engine keeps the list), compiled in a
# window of its own: in the game's sandbox or outside it, as the game will run.
$doWarm = {
    $book = Join-Path $home_ "cache\pipelines\known-pipelines.bin"
    if (-not (Test-Path $book)) { Say "Nothing to build yet: play once first, the game's pipelines are noted while it runs." 'warn'; return }
    $sure = [Windows.MessageBox]::Show("Build every shader pipeline ahead?`n`nAbout an hour and a half with the processor busy, and about 10 GB on drive C: for the graphics driver's cache. The window can be closed at any time: the next time goes on where it stopped.`n`nNot needed for places you have already played in: those are made ahead at every start.", "Vilocity", 'YesNo', 'Question')
    if ($sure -ne 'Yes') { return }
    $how = if ($val.sandbox) { "warm" } else { "native warm" }
    Start-Process cmd -ArgumentList '/k', "`"$exe`" $how && echo. && echo Done. Close this window."
}

# --- Tools page: a card each, with what it holds now ----------------------------------------
$doDiscord = { Start-Process "https://discord.gg/mWzP7ejTek" }
$doLogsTrim = {
    $old = @(Get-ChildItem (Join-Path $home_ "logs") -Filter "gta6-*.log" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -Skip 10)
    if (-not $old) { Say "There are no more than 10 logs: nothing to delete."; return }
    if ([Windows.MessageBox]::Show("Delete $($old.Count) old logs?`n`nThe newest 10 are kept.", "Vilocity", 'YesNo', 'Question') -ne 'Yes') { return }
    $old | Remove-Item -Force -ErrorAction SilentlyContinue; Show-Tools; Say "$($old.Count) old logs are deleted; the newest 10 are kept." 'ok'
}
# Clear the cache and the temporary files, of the game and of the launcher: the translated shaders
# (each version of the translator keeps its own, and the old ones are never read again: 98 GB in
# 2.85 million files on one PC), the pipelines an earlier run saved, the logs but the newest 10,
# and what is left in the temporary folders. Kept: saves, backups, settings, keys, the DLSS and
# FSR downloads, and the lists of the shaders and pipelines the game uses (so they are made again
# ahead). On a thread of its own: millions of files take minutes. The folders themselves stay.
$cleanScript = @'
param($home_, $temp)
function Wipe($folder) {
    if (-not [IO.Directory]::Exists($folder)) { return }
    foreach ($e in [IO.Directory]::GetFileSystemEntries($folder)) {
        try { if ([IO.Directory]::Exists($e)) { [IO.Directory]::Delete($e, $true) } else { [IO.File]::Delete($e) } } catch { }
    }
}
$cache = Join-Path $home_ "cache"; $logs = Join-Path $home_ "logs"
# (First the mark that says the shaders are translated: a clearing cut short still leaves a start that translates them again.)
Remove-Item (Join-Path $cache "shaders-done.txt"), (Join-Path $cache "pipelines\host-pipelines.bin"), (Join-Path $cache "pipelines\host-pipelines.for") -Force -ErrorAction SilentlyContinue
Wipe (Join-Path $cache "shaders"); Wipe (Join-Path $cache "tmp")
Get-ChildItem $logs -Filter "gta6-*.log" -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -Skip 10 | Remove-Item -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $logs "shaders-progress.txt"), (Join-Path $logs "warm-progress.txt"), (Join-Path $logs "update.txt"), (Join-Path $logs "launcher.txt"), (Join-Path $home_ "launcher.json.new") -Force -ErrorAction SilentlyContinue
Get-ChildItem $temp -Filter "vilocity-*" -Force -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
"cleared"
'@
$doClean = {
    if ($state.cleaning) { return }
    if ($state.started -or (Get-Process vilocity -ErrorAction SilentlyContinue)) { Say "Close the game first: it is using its cache." 'warn'; return }
    $size = if ($state.tools) { " Up to {0:N1} GB are freed." -f (($state.tools.cache.mb + $state.tools.logs.mb) / 1024) } else { "" }
    $sure = [Windows.MessageBox]::Show("Clear the cache and the temporary files of the game and the launcher?$size`n`nRemoved: the translated shaders, the saved pipelines, the logs but the newest 10, and temporary files. The next start translates the game's shaders again: a few minutes.`n`nKept: your saves and their backups, your settings and keys, the DLSS and FSR downloads.`n`nClearing can take a few minutes.", "Vilocity", 'YesNo', 'Question')
    if ($sure -ne 'Yes') { return }
    $state.cleaning = $true; $toolInfo.cache.Text = "Clearing..."
    Say "Clearing the cache and the temporary files: it can take a few minutes. PLAY waits for it."
    Start-Work $cleanScript @($home_, $env:TEMP) { param($gave)
        $state.cleaning = $false; Show-Tools
        Say "The cache and the temporary files are cleared. The next start translates the game's shaders again." 'ok'
    }
}
$tools = @(
    @{ id = 'help'; glyph = 0xE897; title = 'Ask for help'; text = "A report holds this PC, your settings and the end of the newest log, with your user name taken out. Paste it in the Discord's help channel or a GitHub issue."
       buttons = @(@('Copy a report', $doReport), @('Discord', $doDiscord), @('Issues', $doIssues)) }
    @{ id = 'cache'; glyph = 0xEDA2; title = 'Cache and temporary files'; text = "What the game and the launcher keep for a faster start. Clearing frees the space; the next start then translates the game's shaders again, a few minutes. Saves, settings and keys are never touched. Building every pipeline ahead takes about an hour and a half, once."
       buttons = @(@('Clear them', $doClean), @('Build all pipelines ahead', $doWarm)) }
    @{ id = 'saves'; glyph = 0xE74E; title = 'Saves'; text = "Your saved games and the game's own settings."
       buttons = @(, @('Open the folder', $doSaves)) }
    @{ id = 'backups'; glyph = 0xE81C; title = 'Save backups'; pick = $true; text = "Your saves are copied at each PLAY, and the last 10 copies are kept. Pick one to put it back: your saves of now are copied first."
       buttons = @(@('Put it back', $doRestore), @('Open the folder', $doBackups)) }
    @{ id = 'bench'; glyph = 0xE916; title = 'Benchmark'; text = "Starts the game. Once you are in the world it measures 60 seconds of play (move around as you usually do), then copies the result to share."
       buttons = @(, @('Start', $doBench)) }
    @{ id = 'update'; glyph = 0xE895; title = 'Update Vilocity'; text = "Asks GitHub for a newer Vilocity. If there is one, it is downloaded and the engine is built again, here in the launcher."
       buttons = @(, @('Look for an update', $doUpdate)) }
    @{ id = 'engine'; glyph = 0xE90F; title = 'The engine'; text = "Builds the engine from its source, in a window of its own (cargo build --release -p vilocity). Needed once, and after you change the source yourself."
       buttons = @(, @('Build the engine', $doBuild)) }
    @{ id = 'logs'; glyph = 0xE9D9; title = 'Logs'; text = "One file for each start of the game. The Log page shows the end of the newest."
       buttons = @(@('Open the folder', $doLogs), @('Delete the old ones', $doLogsTrim)) }
)
$toolInfo = @{}; $toolBox = @{}
$toolClick = { param($from) & $from.Tag }
foreach ($t in $tools) {
    $c = New-Object Windows.Controls.Border; $c.Style = $w.FindResource('Card'); $c.Width = 356; $c.Margin = "0,0,16,16"
    $s = New-Object Windows.Controls.DockPanel; $s.LastChildFill = $false; $c.Child = $s
    $head = New-Object Windows.Controls.StackPanel; $head.Orientation = 'Horizontal'; $head.Margin = "0,0,0,12"
    $badge = New-Object Windows.Controls.Border; $badge.Width = 34; $badge.Height = 34; $badge.CornerRadius = 9; $badge.Background = Brush "#1B202B"; $badge.Margin = "0,0,12,0"
    $g = New-Object Windows.Controls.TextBlock; $g.Text = [string][char]$t.glyph; $g.FontFamily = 'Segoe MDL2 Assets'; $g.FontSize = 16; $g.Foreground = $w.FindResource('Brand'); $g.HorizontalAlignment = 'Center'; $g.VerticalAlignment = 'Center'
    $badge.Child = $g
    $title = New-Object Windows.Controls.TextBlock; $title.Style = $w.FindResource('H'); $title.Text = $t.title; $title.Margin = "0"; $title.VerticalAlignment = 'Center'
    [void]$head.Children.Add($badge); [void]$head.Children.Add($title)
    # (The buttons sit at the card's foot, so those of a row of cards line up.)
    $row = New-Object Windows.Controls.WrapPanel
    foreach ($b in $t.buttons) { $btn = New-Object Windows.Controls.Button; $btn.Content = $b[0]; $btn.Tag = $b[1]; $btn.Margin = "0,0,8,0"; $btn.Add_Click($toolClick); [void]$row.Children.Add($btn) }
    $info = New-Object Windows.Controls.TextBlock; $info.TextWrapping = 'Wrap'; $info.FontSize = 14; $info.Margin = "0,0,0,6"; $info.Text = "Looking..."; $toolInfo[$t.id] = $info
    $text = New-Object Windows.Controls.TextBlock; $text.Style = $w.FindResource('Dim'); $text.Text = $t.text; $text.Margin = "0,0,0,14"
    $parts = @(@($head, 'Top'), @($row, 'Bottom'))
    if ($t.pick) { $box = New-Object Windows.Controls.ComboBox; $box.Margin = "0,0,0,12"; $toolBox[$t.id] = $box; $parts += , @($box, 'Bottom') }
    $parts += @(@($info, 'Top'), @($text, 'Top'))
    foreach ($part in $parts) { [Windows.Controls.DockPanel]::SetDock($part[0], $part[1]); [void]$s.Children.Add($part[0]) }
    [void]$ui.Tools.Children.Add($c)
}
# What the folders hold, counted on a thread of its own, by .NET's own lister: the shader cache of
# a PC that has played holds millions of files (2.85 million here: 86 s with Get-ChildItem, 4 s so).
$toolScan = @'
param($home_)
if (-not ("DirSum" -as [type])) { Add-Type 'using System.IO; public static class DirSum { public static long[] Of(string p) { long n = 0, b = 0; try { if (Directory.Exists(p)) foreach (var f in new DirectoryInfo(p).EnumerateFiles("*", SearchOption.AllDirectories)) { n++; b += f.Length; } } catch { } return new long[] { n, b }; } }' }
$sum = { param($folders) $n = 0; $b = 0; foreach ($f in $folders) { $r = [DirSum]::Of($f); $n += $r[0]; $b += $r[1] }; @{ n = $n; mb = $b / 1MB } }
$logs = @(Get-ChildItem (Join-Path $home_ "logs") -Filter "gta6-*.log" -File -ErrorAction SilentlyContinue)
@{
    saves = & $sum @(Get-ChildItem (Join-Path $home_ "data") -Directory -ErrorAction SilentlyContinue | ForEach-Object { Join-Path $_.FullName "saves" })
    logs = @{ n = $logs.Count; mb = ($logs | Measure-Object Length -Sum).Sum / 1MB }
    cache = & $sum @(Join-Path $home_ "cache")
    backups = @(Get-ChildItem (Join-Path $home_ "backups") -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | ForEach-Object { $_.Name })
}
'@
function Show-Tools {
    $toolInfo.help.Text = "The Discord's help channel, or an issue on GitHub"
    $toolInfo.engine.Text = if (Test-Path $exe) { "Built {0:yyyy-MM-dd HH:mm}" -f (Get-Item $exe).LastWriteTime } else { "Not built yet" }
    $toolInfo.update.Text = "$($ui.Version.Text): " + $(if ($ui.UpdateText.Text -eq 'Update') { "not asked yet" } else { $ui.UpdateText.Text.ToLower() })
    $toolInfo.bench.Text = if ($set.bench) { [string]$set.bench } else { "Not run yet" }
    $toolInfo.bench.FontSize = if ($set.bench) { 12.5 } else { 14 }
    Start-Work $toolScan @($home_) { param($gave)
        $t = if ($gave) { $gave[-1] }; if (-not $t) { return }
        $state.tools = $t
        $toolInfo.saves.Text = if ($t.saves.n) { "{0} files, {1:N1} MB" -f $t.saves.n, $t.saves.mb } else { "No saves yet" }
        $toolInfo.logs.Text = if ($t.logs.n) { "{0} logs, {1:N1} GB" -f $t.logs.n, ($t.logs.mb / 1024) } else { "No log yet" }
        $toolInfo.cache.Text = if ($t.cache.n) { "{0:N1} GB in {1:N0} files, kept for a faster start" -f ($t.cache.mb / 1024), $t.cache.n } else { "Nothing kept yet: play once" }
        $toolInfo.backups.Text = if ($t.backups) { "$(@($t.backups).Count) $(if (@($t.backups).Count -eq 1) { 'copy' } else { 'copies' }) kept" } else { "None yet: one is made at each PLAY" }
        $b = $toolBox.backups; $b.Items.Clear(); foreach ($n in $t.backups) { [void]$b.Items.Add([string]$n) }
        $b.IsEnabled = [bool]$b.Items.Count; if ($b.Items.Count) { $b.SelectedIndex = 0 }
    }
}

# The game's F1 menu writes menu.txt as soon as something changes there: shown here within a second.
$state.menuSeen = (Get-Item $menuFile -ErrorAction SilentlyContinue).LastWriteTimeUtc
$menuWatch = New-Object Windows.Threading.DispatcherTimer; $menuWatch.Interval = [TimeSpan]::FromSeconds(1)
$menuWatch.Add_Tick($safeTick); $menuWatch.Tag = {
    $t = (Get-Item $menuFile -ErrorAction SilentlyContinue).LastWriteTimeUtc
    if ($t -eq $state.menuSeen) { return }
    $state.menuSeen = $t; $m = Read-Menu
    foreach ($o in $opt.Values) { if ($o.store -eq 'menu') { $val[$o.id] = Stored-Value $o $m; Show-Control $o.id } }
    Show-Fit; Show-Summary
}
if (-not $Shot) { $menuWatch.Start() }

# What PLAY runs: the options as the play script's switches and the engine's variables; what "Every
# engine setting" names comes last and wins.
function Get-PlayCommand {
    $inv = [Globalization.CultureInfo]::InvariantCulture
    $vars = [ordered]@{
        VILOCITY_FRAME_INTERVAL_MS = @{ 0 = "0"; 60 = "16.667"; 30 = "33.333" }[[int]$val.rate]
        VILOCITY_FSR_SHARPNESS = ([double]$val.sharp).ToString($inv)
        VILOCITY_HUD = $(if ($val.hud) { "1" } else { "0" })
    }
    if ($opt.Contains('fsr') -and -not $val.fsr) { $vars.VILOCITY_FSR_DLL = "off" }
    if ([double]$val.night -gt 0) { $vars.VILOCITY_AMBIENT_FLOOR = ([double]$val.night).ToString($inv) }
    if ($val.steady) { $vars.VILOCITY_SUBMIT_DEPTH = "1" }
    if ($val.full) { $vars.VILOCITY_FULLSCREEN = "1" }
    if (-not $val.mousekeep) { $vars.VILOCITY_MOUSE = "0" }
    if ($val.helper) { $vars.VILOCITY_HELPER_PRIORITY = "1" }
    if ([double]$val.texpass -gt 0) { $vars.VILOCITY_TEXTURE_PASS_MS = "$([int]$val.texpass)" }
    $extra = (Parse-Extra ([string]$set.extra)).vars; foreach ($k in $extra.Keys) { $vars[$k] = $extra[$k] }
    $sets = ($vars.GetEnumerator() | ForEach-Object { "`$env:$($_.Key) = '$($_.Value -replace "'", "''")'; " }) -join ''
    # (DLSS and frame generation only where their files are; frame generation brings DLSS with it.)
    $frames = $opt.Contains('framegen') -and $opt.framegen.control.IsEnabled -and [double]$val.framegen -ge 2
    $dlss = if ($opt.Contains('upscaler') -and $state.dlss) { @('off', 'quality', 'balanced', 'performance', 'ultraperformance', 'dlaa')[[math]::Max(0, [math]::Min(5, [int]$val.upscaler))] } else { 'off' }
    if ($dlss -eq 'off' -and $frames -and $nvidiaCard) { $dlss = 'quality' }
    $switches = "-NoMonitor" + " -Picture " + @("1080p", "1440p", "4k")[[math]::Min(2, [int]$val.picture)] + $(if (-not $val.sandbox) { " -NoSandbox" }) + $(if ($val.logwin) { " -LogWindow" }) + $(if ($val.measure -and (Test-Path (Join-Path $root "target\diag\vilocity.exe"))) { " -Diag" }) + $(if ($val.rays) { " -RayTracing" }) + $(if ($dlss -ne 'off') { " -Dlss $dlss" }) + $(if ($frames) { " -FrameGen" })
    $play = Join-Path $scripts "play-gta6.ps1"
    $cmd = "$sets & '$($play -replace "'", "''")' -Game '$($ui.Game.Text.Trim().Trim('"') -replace "'", "''")' $switches"
    $cmd
}
$ui.Play.Add_Click({
    if ($state.cleaning) { Say "The cache is being cleared: press PLAY when it is done." 'warn'; return }
    if ($state.started) {
        # The play script with all it started (its step, the game, the panel, the log window): ending
        # only vilocity.exe let the script go on to its next step, and the game started all the same.
        if ($state.proc -and -not $state.proc.HasExited) { & taskkill.exe /PID $state.proc.Id /T /F 2>&1 | Out-Null }
        Get-Process vilocity -ErrorAction SilentlyContinue | Stop-Process -Force
        $state.started = $null; $state.proc = $null; $state.bench = $null; $ui.Progress.Visibility = 'Collapsed'; Show-Play $false; Start-Checks -Keep; Say "Stopped."
        return
    }
    # A game that is there already (started before this window was opened): this window follows it, and starts no second one.
    if (Game-Process) { $state.started = Get-Date; $state.seen = $true; $state.seenAt = (Get-Date).AddHours(-1); Show-Play $true; Say "The game is running already." 'ok'; return }
    Save-Set
    # Your saves, copied before each start (Tools, Save backups).
    try { $null = Backup-Saves } catch {}
    # Smooth Motion into the driver's profile for vilocity.exe, when the switch differs from what the driver holds.
    $note = ""
    if ($opt.Contains('nvsmooth') -and [bool]$val.nvsmooth -ne $smooth.driver) {
        try { $note = " (" + [NvSmooth]::Set('vilocity.exe', 'Vilocity (vilocity.exe)', [bool]$val.nvsmooth) + ")"; $smooth.driver = [bool]$val.nvsmooth }
        catch { $note = " (Smooth Motion was not changed: $($_.Exception.InnerException.Message))" }
    }
    $cmd = Get-PlayCommand
    $state.proc = Start-Process powershell -PassThru -WindowStyle Hidden -ArgumentList "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "`"$($cmd -replace '"', '\"')`""
    $state.started = Get-Date; $state.seen = $false; $state.seenAt = $null; Show-Play $true; Say "Starting...$note"
})

# The game's state, once a second: translating shaders, running (with the frame rate of its window's title), closed.
$timer = New-Object Windows.Threading.DispatcherTimer; $timer.Interval = [TimeSpan]::FromSeconds(1)
$timer.Add_Tick($safeTick); $timer.Tag = {
    if (-not $state.started) { return }
    $g = Game-Process
    $bar = $null
    if ($g) {
        if ($val.leave -and -not $state.seen -and $null -eq $state.bench) { $w.Close(); return }
        # The launcher costs the game nothing while it runs: minimized (WPF draws nothing then) and at low priority.
        if (-not $state.seen) {
            $state.seenAt = Get-Date
            $w.WindowState = 'Minimized'
            try { [Diagnostics.Process]::GetCurrentProcess().PriorityClass = 'BelowNormal' } catch {}
        }
        $state.seen = $true
        Say $(if ($g.MainWindowTitle -match '([\d.]+) fps ([\d.]+) ms/frame') { "Running: {0} fps ({1} ms a frame). F1: the game's settings. F8 hides the panel." -f $Matches[1], $Matches[2] } else { "Running." }) 'ok'
        # The benchmark: the window's frame rate once a second while the world is drawn (over 2,000 draws).
        if ($null -ne $state.bench -and $g.MainWindowTitle -match '([\d.]+) fps [\d.]+ ms/frame (\d+) draws' -and [int]$Matches[2] -gt 2000) {
            $state.bench.Add([double]$Matches[1]); $state.benchDraws += [int]$Matches[2]
            $bar = $state.bench.Count / 60.0
            Say ("Benchmark: {0} of 60 seconds measured. Move around as you usually play." -f $state.bench.Count) 'ok'
            if ($state.bench.Count -ge 60) { & $benchDone }
        } elseif ($null -ne $state.bench) { Say "Benchmark: waiting for the world (it measures once you are in it)..." }
    } elseif (-not $state.seen -and $state.proc -and -not $state.proc.HasExited) {
        # The play script is still at its steps before the game; it is not "did not start" while it runs
        # (said after 45 seconds, PLAY came back and a second press started a second game).
        $step = (Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" | Select-Object -ExpandProperty CommandLine) -join ' '
        Say $(if ($step -like '*native shaders*') {
                # How far it is: a line each 1,000 shaders (the play script keeps them), of as many as the last time.
                $last = Get-Content (Join-Path $home_ "logs\shaders-progress.txt") -Tail 1 -ErrorAction SilentlyContinue
                $all = [int](Get-Content (Join-Path $home_ "cache\shaders-count.txt") -ErrorAction SilentlyContinue | Select-Object -First 1)
                if ($last -match '^(\d+) shaders in the cache, (\d+) s') {
                    if ($all -gt 0) { $bar = [int]$Matches[1] / [double]$all }
                    "Translating the game's shaders: {0:N0}{1} in {2} s (once per build; a few minutes on most PCs)..." -f [int]$Matches[1], $(if ($all -gt 0) { " of about {0:N0}" -f $all } else { "" }), $Matches[2]
                } else { "Translating the game's shaders (once per build; a few minutes on most PCs)..." }
            }
            elseif ($step -like '*warm*') {
                # How far it is, from the lines the play script keeps (a few minutes after an update of Vilocity or the driver).
                $lines = Get-Content (Join-Path $home_ "logs\warm-progress.txt") -Tail 3 -ErrorAction SilentlyContinue
                $done = $lines | Where-Object { $_ -match '^(\d+) of (\d+)' } | Select-Object -Last 1
                if ($done -match '^(\d+) of (\d+)') { if ([int]$Matches[2] -gt 0) { $bar = [int]$Matches[1] / [double]$Matches[2] }; "Preparing the game's pipelines: {0:N0} of {1:N0} (after an update, a few minutes)..." -f [int]$Matches[1], [int]$Matches[2] }
                elseif (($lines | Where-Object { $_ -match '\d+ to make' } | Select-Object -Last 1) -match '(\d+) to make') { "Preparing {0:N0} of the game's pipelines (after an update, a few minutes)..." -f [int]$Matches[1] }
                else { "Preparing the game's pipelines (after an update, a few minutes)..." }
            }
            else { "Starting..." })
    } elseif ($state.seen -or ((Get-Date) - $state.started).TotalSeconds -gt 45) {
        $newest = Get-ChildItem (Join-Path $home_ "logs\gta6-*.log") -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1
        # A game that ended within a minute of its window, with the driver's exception in its log (the play script removed the saved pipelines).
        $quick = -not $state.seen -or ($state.seenAt -and ((Get-Date) - $state.seenAt).TotalSeconds -lt 60)
        $driver = $quick -and $newest -and (((Get-Content $newest.FullName -Raw -ErrorAction SilentlyContinue) -replace "`r?`n", '') -match 'exception 0x[0-9a-f]+ at (amdxc64|nvwgf2umx|igd12umd64)\.dll')
        $how = if ($driver) { "The graphics driver crashed reading the pipelines saved by an earlier run. They are removed now: press PLAY again (it starts slower once)." }
            elseif ($state.seen) { "The game closed." }
            elseif ($val.sandbox -and $newest -and (Select-String -Path $newest.FullName -Pattern '(os error 5)' -SimpleMatch -Quiet)) { "Windows refused the sandbox a folder (access denied): turn Sandbox off in Settings to play, and please report it (Tools, Copy a report)." }
            else { "The game did not start: see the Log page, or Tools, Copy a report to ask for help." }
        if ($null -ne $state.bench) { $state.bench = $null; $how += " (The benchmark ended with the game, before 60 seconds of the world.)" }
        Say $how $(if ($state.seen -and -not $driver) { 'idle' } else { 'bad' })
        if ($state.seen) {
            try { [Diagnostics.Process]::GetCurrentProcess().PriorityClass = 'Normal' } catch {}
            $w.WindowState = 'Normal'; [void]$w.Activate()
        }
        # (The line just said stays: the check that follows does not write "Ready." over it.)
        $state.started = $null; Show-Play $false; Start-Checks -Keep
    }
    $ui.Progress.Visibility = if ($null -ne $bar) { 'Visible' } else { 'Collapsed' }
    if ($null -ne $bar) { $ui.Progress.Value = [math]::Min(1.0, $bar) }
}
$timer.Start()
$w.Add_Closing({ Save-Set })

Show-Needs; Show-Presets; [void](Show-Engine)
# A game that runs already (the launcher was closed and opened again): followed, not started twice.
if (-not $Shot -and -not $Check -and (Game-Process)) { $state.started = Get-Date; $state.seen = $true; $state.seenAt = (Get-Date).AddHours(-1); Show-Play $true }
Start-Checks

if ($Shot) {
    # Draws the window as it is to a picture (for looking at the layout) once this PC is checked, then leaves without saving.
    if ($Page) { $ui["Nav$Page"].IsChecked = $true }
    $shoot = New-Object Windows.Threading.DispatcherTimer; $shoot.Interval = [TimeSpan]::FromMilliseconds(400)
    $shoot.Add_Tick($safeTick); $shoot.Tag = {
        if ($jobs.Count) { return }
        if ($Find -and $ui.Search.Text -ne $Find) { $ui.Search.Text = $Find; return }
        $shoot.Stop()
        $bmp = New-Object Windows.Media.Imaging.RenderTargetBitmap ([int]$w.Content.ActualWidth), ([int]$w.Content.ActualHeight), 96, 96, ([Windows.Media.PixelFormats]::Pbgra32)
        $bmp.Render($w.Content); $enc = New-Object Windows.Media.Imaging.PngBitmapEncoder; $enc.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($bmp))
        $s = [IO.File]::Create($Shot); $enc.Save($s); $s.Close(); $timer.Stop()
        Write-Host "PLAY would run: $(Get-PlayCommand)"; $w.Close()
    }
    $w.Add_ContentRendered({ $shoot.Start() })
}
if ($Check) {
    # The launcher's own logic, tried in its own folder: each line is a thing that must hold.
    function Expect($ok, [string]$what) { if (-not $ok) { [void]$fail.Add($what) } }
    $back = Decode-Setup ("someone wrote: " + (Encode-Setup @{ picture = 1; light = 1.2; full = $true }))
    Expect ($back -and $back.picture -eq 1 -and [double]$back.light -eq 1.2 -and $back.full -eq $true) "a setup comes back whole from its line of text"
    Expect ($null -eq (Decode-Setup "hello") -and $null -eq (Decode-Setup "VIL1:!!!!") -and $null -eq (Decode-Setup "VIL1:aGVsbG8=")) "text that is no setup is refused"
    $taken = Apply-Setup @{ sandbox = $false; nonsense = 3; picture = 7; light = 0.8; full = "yes"; rate = $true }
    Expect ($val.sandbox -eq $true) "a setup cannot turn the sandbox off"
    Expect ([double]$val.picture -ne 7 -and $val.full -is [bool] -and [double]$val.rate -eq 60) "a value that is no choice is refused"
    Expect ($taken -eq 1 -and [double]$val.light -eq 0.8 -and (Read-Menu).light -eq 0.8) "a setup's good values are taken and kept in menu.txt"
    Set-Option 'rate' 30; Set-Option 'haze' $false
    Expect ((Get-Content $file -Raw | ConvertFrom-Json).rate -eq 30) "a choice is kept in launcher.json as soon as it is made"
    Expect ((Read-Menu).haze -eq 1 -and (Stored-Value $opt.haze (Read-Menu)) -eq $false) "heat haze off is 1 in menu.txt, as the game's menu has it"
    Reset-Options
    Expect ([double]$val.rate -eq 60 -and [double]$val.light -eq 1.2 -and -not (Read-Menu).ContainsKey('light')) "back to Vilocity's own forgets what was set"
    $x = Parse-Extra "VILOCITY_A=1; bad; VILOCITY_B = two words ;"
    Expect ($x.vars.Count -eq 2 -and $x.vars.VILOCITY_B -eq "two words" -and $x.bad.Count -eq 1) "engine settings by name are read, and what is not one is left out"
    $keys = '[general]', 'SWITCH = G', '[walk]', '# a note', 'A = Space, Enter', 'B = R', '[drive]', 'A = Space'
    $e = Edit-Keys $keys.Clone() 'walk' 'B' 'Space' $null
    Expect ($e.lines[4] -eq 'A = Enter' -and $e.lines[5] -eq 'B = R, Space' -and $e.lines[7] -eq 'A = Space' -and $e.lines[3] -eq '# a note') "a key moves to its new action in its own layout only"
    Expect ((Edit-Keys $keys.Clone() 'walk' 'B' 'G' $null).error) "the switch key is not given to an action"
    $both = '[general]', 'ENTER_EXIT = F', 'SWITCH = G', '[walk]', 'Y = F, E', 'A = H'
    $e = Edit-Keys $both.Clone() 'general' 'ENTER_EXIT' 'E' $null
    Expect ($e.lines[1] -eq 'ENTER_EXIT = F, E' -and $e.lines[4] -eq 'Y = F, E') "the get-in key stays an action's key too"
    $e = Edit-Keys $both.Clone() 'general' 'SWITCH' 'H' $null
    Expect ($e.lines[2] -eq 'SWITCH = G, H' -and $e.lines[5] -eq 'A = ' -and -not (Edit-Keys $both.Clone() 'walk' 'A' 'F' $null).error) "the switch takes its key from the actions, and the get-in key can be an action's"
    if ($keyLabels.walk.Count) { Expect ((Key-Label 'walk' 'B') -eq 'Crouch' -and (Key-Label 'drive' 'A') -eq 'Handbrake' -and (Key-Label 'walk' 'MENU') -eq 'Pause') "the actions have the game's own names" }
    Expect ((Edit-Keys $keys.Clone() 'walk' 'A' $null 'Space').lines[4] -eq 'A = Enter') "a key is taken away"
    Expect ((Key-Name 'D7') -eq '7' -and (Key-Name 'NumPad3') -eq 'Num3' -and (Key-Name 'OemComma') -eq 'Comma' -and (Key-Name 'F9') -eq 'F9' -and -not (Key-Name 'F8') -and -not (Key-Name 'F3') -and -not (Key-Name 'LeftAlt') -and -not (Key-Name 'RightAlt')) "keys get the keys file's names, and Vilocity's own keys none"
    Set-Engine 'VILOCITY_A' '2'; Set-Engine 'VILOCITY_B' 'x y'; Set-Engine 'VILOCITY_A' ''
    Expect ($set.extra -eq 'VILOCITY_B=x y' -and (Get-Content $file -Raw | ConvertFrom-Json).extra -eq 'VILOCITY_B=x y') "an engine setting is set by name, kept, and forgotten when emptied"
    $ui.Search.Text = "fog light"
    Expect (@($rows | Where-Object { $_.row.Visibility -eq 'Visible' }).Count -eq 1 -and $ui.NoMatch.Visibility -eq 'Collapsed') "a word finds its setting"
    $ui.Search.Text = ""
    Set-Option 'rate' 30; Set-Option 'full' $false; Set-Option 'picture' 1; Set-Engine 'VILOCITY_B' "it's"
    $run = Get-PlayCommand
    Expect ($run -like "*`$env:VILOCITY_FRAME_INTERVAL_MS = '33.333'; *" -and $run -notlike '*VILOCITY_FULLSCREEN*' -and $run -like "*`$env:VILOCITY_B = 'it''s'; *" -and $run -like '* -NoMonitor -Picture 1440p*' -and $run -notlike '*-NoSandbox*' -and $run -notlike '*-Dlss*') "PLAY runs the play script with the options chosen"
    Set-Engine 'VILOCITY_B' ''; Reset-Options
    # The window's own parts, pressed as a player would: a list, a switch, a setup, the keys.
    $press = { param($button) $button.RaiseEvent((New-Object Windows.RoutedEventArgs ([Windows.Controls.Primitives.ButtonBase]::ClickEvent))) }
    $opt.light.control.SelectedIndex = 4
    Expect ([double]$val.light -eq 1.0 -and (Read-Menu).light -eq 1.0) "choosing in a list sets the option"
    $opt.full.control.IsChecked = $false; & $press $opt.full.control
    Expect ($val.full -eq $false -and (Get-Content $file -Raw | ConvertFrom-Json).full -eq $false) "a switch sets the option"
    & $press (@($ui.Presets.Children) | Where-Object { $_.Tag -eq 'Sharpest' })
    Expect ([double]$val.picture -eq 1 -and [double]$val.light -eq 1.2 -and $opt.picture.control.SelectedIndex -eq 1) "a setup sets its options, and the lists show them"
    $ui.PresetName.Text = "Mine"; & $press $ui.PresetSave
    Expect ((Get-Content $file -Raw | ConvertFrom-Json).presets.Mine.picture -eq 1 -and @($ui.Presets.Children | Where-Object { $_.Tag -eq 'Mine' }).Count -eq 2) "your own setup is kept and shown, with its delete button"
    if ($opt.Contains('mouse')) {
        Expect ($ui.MouseRows.Children.Count -ge 3 -and $opt.mouse.controls.Count -eq 2) "the mouse's settings are on the Keys and mouse page too"
        $opt.mouse.controls[1].SelectedIndex = 6
        Expect ([double]$val.mouse -eq 2.0 -and (Read-Menu).mouse -eq 2.0 -and $opt.mouse.controls[0].SelectedIndex -eq 6) "the mouse's speed set on one page shows on the other, and is kept"
    }
    [IO.File]::WriteAllLines($keysFile, [string[]]$keys); $ui.Nav2.IsChecked = $true
    $walk = @($ui.KeysA.Children)[1].Child
    & $press (@(@($walk.Children)[1].Children)[1].Children | Where-Object { $_.Content -eq '+' })
    Expect ($state.capture.name -eq 'A') "the + of an action waits for its key"
    & $keyGot 'K'
    Expect ((Get-Content $keysFile)[4] -eq 'A = Space, Enter, K' -and -not $state.capture) "the key pressed is written to the keys file"
    & $press (@(@(@($ui.KeysA.Children)[1].Child.Children)[1].Children)[1].Children | Where-Object { $_.Content -eq 'Enter' })
    Expect ((Get-Content $keysFile)[4] -eq 'A = Space, K') "a key clicked is taken away"
    & $press (@($ui.Presets.Children) | Where-Object { $_.Tag -eq 'Fastest' })
    Expect ([double]$val.picture -eq 0 -and @($ui.Presets.Children | Where-Object { $_.Tag -eq 'Undo' }).Count -eq 1) "a setup taken leaves an Undo beside the setups"
    & $press (@($ui.Presets.Children) | Where-Object { $_.Tag -eq 'Undo' })
    Expect ([double]$val.picture -eq 1 -and -not $state.undo) "Undo puts the settings back as they were"
    # The cache cleared: what is to go goes, what is to stay stays.
    $c = Join-Path $home_ "cache"; $fakeTemp = Join-Path $home_ "temp"
    New-Item -ItemType Directory -Force (Join-Path $c "shaders\ab"), (Join-Path $c "pipelines"), (Join-Path $c "tmp\x"), (Join-Path $home_ "data\t\saves"), (Join-Path $fakeTemp "vilocity-old") | Out-Null
    foreach ($f in "cache\shaders\ab\1", "cache\shaders-done.txt", "cache\pipelines\host-pipelines.bin", "cache\pipelines\known-pipelines.bin", "cache\tmp\x\y", "data\t\saves\s", "temp\other.txt") { Set-Content (Join-Path $home_ $f) "x" }
    1..12 | ForEach-Object { Set-Content (Join-Path $home_ ("logs\gta6-{0:00}.log" -f $_)) "12:00:0$($_ % 10) INFO a line`n12:00:01 WARN a warning" }
    [void](& ([scriptblock]::Create($cleanScript)) $home_ $fakeTemp)
    $has = { param($f) Test-Path (Join-Path $home_ $f) }
    Expect ((& $has "cache\shaders") -and -not (& $has "cache\shaders\ab") -and -not (& $has "cache\shaders-done.txt") -and -not (& $has "cache\pipelines\host-pipelines.bin") -and -not (& $has "cache\tmp\x") -and -not (& $has "temp\vilocity-old")) "clearing removes the shaders, the saved pipelines and the temporary files"
    Expect ((& $has "cache\pipelines\known-pipelines.bin") -and (& $has "data\t\saves\s") -and (& $has "launcher.json") -and (& $has "data\keys.txt") -and (& $has "temp\other.txt") -and @(Get-ChildItem (Join-Path $home_ "logs") -Filter "gta6-*.log").Count -eq 10) "clearing keeps the saves, the settings, the keys, the lists and the newest 10 logs"
    New-Item -ItemType Directory -Force (Join-Path $home_ "drive\Games\GTA") | Out-Null; Set-Content (Join-Path $home_ "drive\Games\GTA\game_scarlett_master.exe") "x"
    Expect ((& ([scriptblock]::Create($gameScan)) @((Join-Path $home_ "drive"))) -eq (Join-Path $home_ "drive\Games\GTA")) "the game's folder is found on a drive"
    $ui.Nav5.IsChecked = $true; $ui.LogBad.IsChecked = $true; & $showLogLines
    Expect ($ui.LogBox.Text -eq "12:00:01 WARN a warning" -and $ui.LogName.Text -like '*1 of its last 2 lines') "the log page shows the newest log, and only its warnings when asked"
    $ui.LogBad.IsChecked = $false
    $ui.Nav4.IsChecked = $true
    Expect ($ui.Tools.Children.Count -eq 8 -and $toolInfo.engine.Text -match 'uilt') "the tools page has its cards"
    $ui.Nav0.IsChecked = $true
    # And in the window itself: an error thrown in a timer's code must leave it open, and this PC must get checked.
    $state.ticks = 0; $prove = New-Object Windows.Threading.DispatcherTimer; $prove.Interval = [TimeSpan]::FromMilliseconds(300)
    $prove.Add_Tick($safeTick); $prove.Tag = {
        $state.ticks++
        if ($state.ticks -eq 1) { $w.Dispatcher.BeginInvoke([Action] { throw "the check's own error, in a handler" }); throw "the check's own error, in a timer" }
        if ($state.ticks -eq 2) { Expect ($ui.Status.Text -like '*keeps running*') "an error is said at the bottom of the window" }
        if ((-not $state.pc -or -not $state.names) -and $state.ticks -lt 200) { return }
        $prove.Stop()
        Expect ($state.names.Count -gt 50 -and (Show-Engine 'frame_interval') -ge 1 -and $state.names['VILOCITY_FRAME_INTERVAL_MS']) "the engine's settings are read from its source, with their notes"
        Expect (@($state.pc.engine).Count -gt 5 -and (Show-Engine) -gt 5) "what the play script sets on this PC is listed"
        Expect ($ui.Specs.Children.Count -eq 4 -and $state.pc.cpu -and $state.pc.ram -gt 1 -and $ui.BestText.Text -like 'The 1*') "this PC's tiles and its best settings are shown"
        $real = $state.pc
        $state.pc = @{ cardTotal = 7.9; cardFree = 7.0; ram = 15.8 }; $small = (Best-Setup).setup.picture
        $state.pc = @{ cardTotal = 15.9; cardFree = 15.0; ram = 62 }; $large = (Best-Setup).setup.picture
        $state.pc = @{ cardTotal = 15.9; cardFree = 9.0; ram = 62 }; $busy = (Best-Setup).setup.picture
        $state.pc = $real
        Expect ($small -eq 0 -and $large -eq 1 -and $busy -eq 0) "the best picture follows the card and the memory: 1440p only where they take it now"
        Expect ($state.errors -eq 2 -and $w.IsVisible) "an error in a handler or a timer is caught, the window stays and the timer ticks on"
        Expect ($state.pc -and $state.pc.verdict.text -and $ui.Checks.Children.Count -ge 4 -and $ui.FitText.Text -notlike 'Checking*') "this PC is checked on a thread of its own"
        $w.Close()
    }
    $w.Add_ContentRendered({ $state.opened = $since.Elapsed.TotalSeconds; $prove.Start() })
}
[void]$w.ShowDialog()
if ($only) { $only.ReleaseMutex() }
if ($Check) {
    if ($fail.Count) { $fail | ForEach-Object { Write-Host "FAILED: $_" }; exit 1 }
    Write-Host ("The launcher's checks all hold. Its window was there {0:N1} s after the start." -f $state.opened)
}

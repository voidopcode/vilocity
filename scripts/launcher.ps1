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

$root = Split-Path $PSScriptRoot
$home_ = Join-Path $env:LOCALAPPDATA "vilocity"
$file = Join-Path $home_ "launcher.json"
$exe = Join-Path $root "target\release\vilocity.exe"

# What the window remembers, with what it starts from. (The game folder: the user's
# variable as it is now, not as this window's parent had it.)
$known = [Environment]::GetEnvironmentVariable('VILOCITY_GTA6_DIR', 'User'); if (-not $known) { $known = $env:VILOCITY_GTA6_DIR }
$set = @{ game = $known; mode = 0; cap = 0; sharp = 0.0; fsr = $true; hud = $true; sandbox = $true; night = $true; full = $true; logwin = $true; rays = $false; dlss = $false; framegen = $false; steady = $false; leave = $false; extra = "" }
if (Test-Path $file) {
    try { $old = Get-Content $file -Raw | ConvertFrom-Json; foreach ($k in @($set.Keys)) { if ($null -ne $old.$k) { $set[$k] = $old.$k } } } catch {}
}
# A remembered folder that is gone (the game was moved): the user's variable, if that one is there.
$there = { param($d) $d -and (Test-Path (Join-Path $d "game_scarlett_master.exe")) }
if (-not (& $there $set.game) -and (& $there $known)) { $set.game = $known }

[xml]$xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Vilocity" Width="1200" Height="800" ResizeMode="CanMinimize" WindowStartupLocation="CenterScreen"
        Background="#0A0B10" Foreground="#EDEFF5" FontFamily="Segoe UI" FontSize="13">
  <Window.Resources>
    <LinearGradientBrush x:Key="Accent" StartPoint="0,0" EndPoint="1,1"><GradientStop Color="#7C5CFF" Offset="0"/><GradientStop Color="#4F8BFF" Offset="1"/></LinearGradientBrush>
    <Style TargetType="TextBlock"><Setter Property="Foreground" Value="#EDEFF5"/></Style>
    <Style x:Key="Dim" TargetType="TextBlock"><Setter Property="Foreground" Value="#8A91A6"/><Setter Property="FontSize" Value="12"/><Setter Property="TextWrapping" Value="Wrap"/></Style>
    <Style x:Key="Head" TargetType="TextBlock"><Setter Property="Foreground" Value="#8A91A6"/><Setter Property="FontSize" Value="11"/><Setter Property="FontWeight" Value="SemiBold"/><Setter Property="Margin" Value="0,0,0,10"/></Style>
    <Style x:Key="Label" TargetType="TextBlock"><Setter Property="Foreground" Value="#EDEFF5"/><Setter Property="Margin" Value="0,0,0,6"/></Style>
    <Style x:Key="Title" TargetType="TextBlock"><Setter Property="Foreground" Value="#EDEFF5"/><Setter Property="FontSize" Value="22"/><Setter Property="FontWeight" Value="SemiBold"/><Setter Property="Margin" Value="0,0,0,16"/></Style>
    <Style x:Key="Card" TargetType="Border"><Setter Property="Background" Value="#141722"/><Setter Property="CornerRadius" Value="14"/><Setter Property="Padding" Value="18,16"/><Setter Property="BorderBrush" Value="#1F2433"/><Setter Property="BorderThickness" Value="1"/></Style>
    <Style TargetType="Button">
      <Setter Property="Foreground" Value="#EDEFF5"/><Setter Property="Background" Value="#1D2130"/><Setter Property="Padding" Value="14,8"/><Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="Button">
        <Border x:Name="b" Background="{TemplateBinding Background}" CornerRadius="9" Padding="{TemplateBinding Padding}" BorderThickness="1" BorderBrush="#2A3042">
          <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/></Border>
        <ControlTemplate.Triggers>
          <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="b" Property="Opacity" Value="0.86"/></Trigger>
          <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#EDEFF5"/></Trigger>
          <Trigger Property="IsEnabled" Value="False"><Setter TargetName="b" Property="Opacity" Value="0.4"/></Trigger>
        </ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
    </Style>
    <!-- A size to choose: one of a row. -->
    <Style x:Key="Pill" TargetType="RadioButton">
      <Setter Property="Foreground" Value="#C6CBDA"/><Setter Property="Margin" Value="0,0,8,0"/><Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="RadioButton">
        <Border x:Name="b" Background="#1D2130" CornerRadius="9" Padding="18,8" BorderThickness="1" BorderBrush="#2A3042"><ContentPresenter/></Border>
        <ControlTemplate.Triggers>
          <Trigger Property="IsChecked" Value="True"><Setter TargetName="b" Property="Background" Value="{StaticResource Accent}"/><Setter TargetName="b" Property="BorderBrush" Value="Transparent"/><Setter Property="Foreground" Value="White"/></Trigger>
          <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#EDEFF5"/></Trigger>
        </ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
    </Style>
    <!-- A page to go to, in the rail on the left. -->
    <Style x:Key="Nav" TargetType="RadioButton">
      <Setter Property="Foreground" Value="#8A91A6"/><Setter Property="Margin" Value="0,0,0,4"/><Setter Property="Cursor" Value="Hand"/><Setter Property="FontSize" Value="14"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="RadioButton">
        <Border x:Name="b" Background="Transparent" CornerRadius="10" Padding="12,10" BorderThickness="1" BorderBrush="Transparent"><ContentPresenter/></Border>
        <ControlTemplate.Triggers>
          <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="b" Property="Background" Value="#141722"/></Trigger>
          <Trigger Property="IsChecked" Value="True"><Setter TargetName="b" Property="Background" Value="#1A1E2C"/><Setter Property="Foreground" Value="#EDEFF5"/></Trigger>
          <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="b" Property="BorderBrush" Value="#EDEFF5"/></Trigger>
        </ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
    </Style>
    <Style x:Key="Glyph" TargetType="TextBlock"><Setter Property="FontFamily" Value="Segoe MDL2 Assets"/><Setter Property="FontSize" Value="15"/><Setter Property="Margin" Value="0,2,12,0"/><Setter Property="Foreground" Value="{Binding Foreground, RelativeSource={RelativeSource AncestorType=RadioButton}}"/></Style>
    <Style x:Key="NavText" TargetType="TextBlock"><Setter Property="Foreground" Value="{Binding Foreground, RelativeSource={RelativeSource AncestorType=RadioButton}}"/></Style>
    <Style TargetType="CheckBox"><Setter Property="Foreground" Value="#EDEFF5"/><Setter Property="Margin" Value="0,0,0,10"/></Style>
    <Style TargetType="TextBox"><Setter Property="Background" Value="#0A0B10"/><Setter Property="Foreground" Value="#EDEFF5"/><Setter Property="BorderBrush" Value="#2A3042"/>
      <Setter Property="Padding" Value="8,6"/><Setter Property="CaretBrush" Value="#EDEFF5"/></Style>
  </Window.Resources>
  <Grid>
    <Grid.ColumnDefinitions><ColumnDefinition Width="210"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions>
    <!-- The rail: where to go. -->
    <Border Background="#0E1017" BorderBrush="#1A1E2C" BorderThickness="0,0,1,0">
      <DockPanel Margin="14,20,14,16">
        <StackPanel DockPanel.Dock="Top" Orientation="Horizontal" Margin="6,0,0,24">
          <Image x:Name="Logo" Width="34" Height="34" Margin="0,0,10,0" RenderOptions.BitmapScalingMode="HighQuality"/>
          <TextBlock Text="Vilocity" FontSize="19" FontWeight="SemiBold" VerticalAlignment="Center"/>
        </StackPanel>
        <TextBlock DockPanel.Dock="Bottom" Style="{StaticResource Dim}" Margin="8,0,0,0" Text="Your own copy of the game. No network, none of your files: the game runs in a sandbox."/>
        <StackPanel>
          <RadioButton x:Name="Nav0" GroupName="nav" Style="{StaticResource Nav}" IsChecked="True"><StackPanel Orientation="Horizontal"><TextBlock Style="{StaticResource Glyph}" Text="&#xE768;"/><TextBlock Style="{StaticResource NavText}" Text="Play"/></StackPanel></RadioButton>
          <RadioButton x:Name="Nav1" GroupName="nav" Style="{StaticResource Nav}"><StackPanel Orientation="Horizontal"><TextBlock Style="{StaticResource Glyph}" Text="&#xE7F8;"/><TextBlock Style="{StaticResource NavText}" Text="This PC"/></StackPanel></RadioButton>
          <RadioButton x:Name="Nav2" GroupName="nav" Style="{StaticResource Nav}"><StackPanel Orientation="Horizontal"><TextBlock Style="{StaticResource Glyph}" Text="&#xE713;"/><TextBlock Style="{StaticResource NavText}" Text="Options"/></StackPanel></RadioButton>
          <RadioButton x:Name="Nav3" GroupName="nav" Style="{StaticResource Nav}"><StackPanel Orientation="Horizontal"><TextBlock Style="{StaticResource Glyph}" Text="&#xE90F;"/><TextBlock Style="{StaticResource NavText}" Text="Tools"/></StackPanel></RadioButton>
          <RadioButton x:Name="Nav4" GroupName="nav" Style="{StaticResource Nav}"><StackPanel Orientation="Horizontal"><TextBlock Style="{StaticResource Glyph}" Text="&#xE9D9;"/><TextBlock Style="{StaticResource NavText}" Text="Log"/></StackPanel></RadioButton>
        </StackPanel>
      </DockPanel>
    </Border>
    <Grid Grid.Column="1" Margin="28,22,28,22">
      <!-- Play: the game, its picture, go. -->
      <ScrollViewer x:Name="Page0" VerticalScrollBarVisibility="Auto"><StackPanel>
        <Border CornerRadius="18" Height="290" Margin="0,0,0,14">
          <Border.Background><LinearGradientBrush StartPoint="0,0" EndPoint="1,1"><GradientStop Color="#1A1440" Offset="0"/><GradientStop Color="#3A2A9A" Offset="0.6"/><GradientStop Color="#1E5C8A" Offset="1"/></LinearGradientBrush></Border.Background>
          <Grid>
            <Border CornerRadius="18"><Border.Background><ImageBrush x:Name="Hero" Stretch="UniformToFill"/></Border.Background></Border>
            <Border CornerRadius="18"><Border.Background><LinearGradientBrush StartPoint="0,0" EndPoint="1,0"><GradientStop Color="#FA0A0B10" Offset="0"/><GradientStop Color="#E60A0B10" Offset="0.4"/><GradientStop Color="#700A0B10" Offset="0.68"/><GradientStop Color="#000A0B10" Offset="1"/></LinearGradientBrush></Border.Background></Border>
            <StackPanel Margin="30,0,30,0" VerticalAlignment="Center" HorizontalAlignment="Left" MaxWidth="520">
              <TextBlock x:Name="GameName" FontSize="32" FontWeight="Bold" Foreground="White" TextWrapping="Wrap"/>
              <TextBlock x:Name="Status" FontSize="14" Foreground="#D5D8E6" TextWrapping="Wrap" Margin="0,8,0,0"/>
              <StackPanel Orientation="Horizontal" Margin="0,22,0,0">
                <Button x:Name="Play" Content="PLAY" FontSize="17" FontWeight="Bold" Padding="46,13" Background="{StaticResource Accent}" Foreground="White" IsDefault="True"/>
                <Button x:Name="Browse" Content="Game folder..." Margin="12,0,0,0" Background="#33FFFFFF" Padding="16,13"/>
              </StackPanel>
            </StackPanel>
          </Grid>
        </Border>
        <Border Style="{StaticResource Card}" Margin="0,0,0,14">
          <Grid><Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
            <StackPanel>
              <TextBlock Style="{StaticResource Head}" Text="PICTURE"/>
              <StackPanel Orientation="Horizontal"><RadioButton x:Name="Mode0" GroupName="mode" Style="{StaticResource Pill}" Content="1080p" FontSize="15"/><RadioButton x:Name="Mode2" GroupName="mode" Style="{StaticResource Pill}" Content="1440p" FontSize="15"/><RadioButton x:Name="Mode1" GroupName="mode" Style="{StaticResource Pill}" Content="4K" FontSize="15"/></StackPanel>
              <TextBlock x:Name="ModeText" Style="{StaticResource Dim}" Margin="0,10,0,0"/>
            </StackPanel>
            <StackPanel Grid.Column="1" Margin="24,0,0,0" VerticalAlignment="Center">
              <CheckBox x:Name="Full" ToolTip="In a window the picture is shrunk to fit the desktop, which softens it"><TextBlock Text="Full screen (F11 switches)"/></CheckBox>
              <CheckBox x:Name="Hud"><TextBlock Text="Frame rate panel (F8 hides it)"/></CheckBox>
              <CheckBox x:Name="LogWindow" Margin="0" ToolTip="A console that shows what the engine writes while the game runs"><TextBlock Text="Live log window"/></CheckBox>
            </StackPanel>
          </Grid>
        </Border>
        <Border Style="{StaticResource Card}">
          <Grid><Grid.ColumnDefinitions><ColumnDefinition Width="Auto"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions>
            <Border x:Name="CoverBox" Width="44" Height="44" CornerRadius="8" Margin="0,0,14,0" Visibility="Collapsed"><Border.Background><ImageBrush x:Name="Cover" Stretch="UniformToFill"/></Border.Background></Border>
            <StackPanel Grid.Column="1"><TextBlock Style="{StaticResource Head}" Text="GAME FOLDER" Margin="0,0,0,6"/><TextBox x:Name="Game" ToolTip="The folder that holds game_scarlett_master.exe"/></StackPanel>
          </Grid>
        </Border>
        <Border Style="{StaticResource Card}" Margin="0,14,0,0"><StackPanel>
          <TextBlock Style="{StaticResource Head}" Text="KEYS" Margin="0,0,0,6"/>
          <TextBlock Style="{StaticResource Dim}" Text="On foot: W A S D move, the mouse looks, Space jumps, Shift runs, F gets in and out. In a car press G for the driving keys (W accelerates, S brakes, Space is the handbrake) and G again on foot; the frame rate panel says which keys hold. F11: full screen. Esc frees the mouse. All keys: Tools, Keys."/>
        </StackPanel></Border>
      </StackPanel></ScrollViewer>
      <!-- This PC: what it is, what it gets, what is missing. -->
      <ScrollViewer x:Name="Page1" VerticalScrollBarVisibility="Auto" Visibility="Collapsed"><StackPanel>
        <DockPanel><Button x:Name="Recheck" DockPanel.Dock="Right" Content="Check again" VerticalAlignment="Top"/><TextBlock Style="{StaticResource Title}" Text="This PC"/></DockPanel>
        <Border Style="{StaticResource Card}" Margin="0,0,0,14"><StackPanel><TextBlock Style="{StaticResource Head}" Text="SETTINGS CHOSEN FOR THIS PC"/><TextBlock x:Name="Profile" TextWrapping="Wrap" FontSize="14"/></StackPanel></Border>
        <Border Style="{StaticResource Card}"><StackPanel><TextBlock Style="{StaticResource Head}" Text="CHECKS"/><StackPanel x:Name="Checks"/></StackPanel></Border>
      </StackPanel></ScrollViewer>
      <!-- Options: for those who want them. -->
      <ScrollViewer x:Name="Page2" VerticalScrollBarVisibility="Auto" Visibility="Collapsed"><StackPanel>
        <TextBlock Style="{StaticResource Title}" Text="Options"/>
        <Border Style="{StaticResource Card}" Margin="0,0,0,14"><StackPanel>
          <TextBlock Style="{StaticResource Label}" Text="Frame rate"/>
          <StackPanel Orientation="Horizontal" Margin="0,0,0,16"><RadioButton x:Name="Cap0" GroupName="cap" Style="{StaticResource Pill}" Content="No limit"/><RadioButton x:Name="Cap1" GroupName="cap" Style="{StaticResource Pill}" Content="At most 60"/><RadioButton x:Name="Cap2" GroupName="cap" Style="{StaticResource Pill}" Content="At most 30"/></StackPanel>
          <StackPanel Orientation="Horizontal"><TextBlock Text="Sharpening"/><TextBlock x:Name="SharpText" Style="{StaticResource Dim}" Margin="8,1,0,0"/></StackPanel>
          <Slider x:Name="Sharp" Minimum="0" Maximum="1" TickFrequency="0.1" IsSnapToTickEnabled="True" Margin="0,6,0,0"/>
        </StackPanel></Border>
        <Border Style="{StaticResource Card}" Margin="0,0,0,14"><StackPanel>
          <CheckBox x:Name="Dlss" ToolTip="DLSS Super Resolution in place of the game's own upscaling. NVIDIA's Streamline SDK is not part of Vilocity: download it from github.com/NVIDIA-RTX/Streamline/releases and unpack it into %LOCALAPPDATA%\vilocity\streamline"><TextBlock x:Name="DlssText" TextWrapping="Wrap" Text="DLSS (NVIDIA RTX cards): a sharper picture from the same frame"/></CheckBox>
          <CheckBox x:Name="FrameGen" ToolTip="DLSS Frame Generation, with DLSS. The controls answer at the real frame rate"><TextBlock TextWrapping="Wrap" Text="Frame generation (DLSS, RTX 40 and 50): a made-up frame between every two, twice the frame rate shown"/></CheckBox>
          <CheckBox x:Name="Steady" ToolTip="VILOCITY_SUBMIT_DEPTH=1. Measured: 2 to 5 torn frames in 2,600 while walking at night against 2 to 33; the opening at 29.7 frames a second against 31"><TextBlock TextWrapping="Wrap" Text="Steadier picture: fewer frames in which a character is drawn torn (seen at night), about 1.5 ms a frame slower"/></CheckBox>
          <CheckBox x:Name="Rays" ToolTip="VILOCITY_BVH_BUILDER=1. Not measured here on an AMD card; other forks report 10-16 fps with it against 24 without on an RX 7900 XTX"><TextBlock TextWrapping="Wrap" Text="Ray tracing: the game's own traced light. For AMD Radeon cards; on NVIDIA cards it runs at 3 to 5 frames a second"/></CheckBox>
          <CheckBox x:Name="Night" ToolTip="VILOCITY_AMBIENT_FLOOR=0.05: the least light the stand-in for the traced light gives a surface"><TextBlock TextWrapping="Wrap" Text="Brighter nights: a little light where no lamp reaches"/></CheckBox>
          <CheckBox x:Name="Smooth" ToolTip="Written into the NVIDIA driver's profile for vilocity.exe when you press PLAY; nothing else of the driver is changed"><TextBlock TextWrapping="Wrap" Text="NVIDIA Smooth Motion: the driver shows a made-up picture between every two (twice the frame rate shown; RTX 40 and 50)"/></CheckBox>
          <CheckBox x:Name="Fsr"><TextBlock TextWrapping="Wrap" Text="AMD's PC upscaler, if its DLL is installed (a little faster)"/></CheckBox>
          <CheckBox x:Name="Sandbox"><TextBlock TextWrapping="Wrap" Text="Sandbox: no network, none of your files (recommended)"/></CheckBox>
          <CheckBox x:Name="Leave" Margin="0"><TextBlock TextWrapping="Wrap" Text="Close this window when the game starts"/></CheckBox>
        </StackPanel></Border>
        <Border Style="{StaticResource Card}"><StackPanel>
          <TextBlock Style="{StaticResource Label}" Text="Engine settings by name"/>
          <TextBox x:Name="Extra" ToolTip="NAME=VALUE; NAME=VALUE. For example: VILOCITY_AMBIENT=1.0; VILOCITY_THIN_THREADS=12"/>
          <TextBlock Style="{StaticResource Dim}" Margin="0,8,0,0" Text="NAME=VALUE; NAME=VALUE. What is named here wins over what the launcher sets."/>
        </StackPanel></Border>
      </StackPanel></ScrollViewer>
      <!-- Tools. -->
      <ScrollViewer x:Name="Page3" VerticalScrollBarVisibility="Auto" Visibility="Collapsed"><StackPanel>
        <TextBlock Style="{StaticResource Title}" Text="Tools"/>
        <Border Style="{StaticResource Card}"><StackPanel>
          <DockPanel Margin="0,0,0,12"><Button x:Name="Keys" DockPanel.Dock="Right" Content="Open" MinWidth="110"/><StackPanel><TextBlock Text="Keys"/><TextBlock Style="{StaticResource Dim}" Text="Which key does what. Change it and save: the running game takes it within a second."/></StackPanel></DockPanel>
          <DockPanel Margin="0,0,0,12"><Button x:Name="Saves" DockPanel.Dock="Right" Content="Open" MinWidth="110"/><StackPanel><TextBlock Text="Saves"/><TextBlock Style="{StaticResource Dim}" Text="The folder with your saved games and the game's settings."/></StackPanel></DockPanel>
          <DockPanel Margin="0,0,0,12"><Button x:Name="Logs" DockPanel.Dock="Right" Content="Open" MinWidth="110"/><StackPanel><TextBlock Text="Logs"/><TextBlock Style="{StaticResource Dim}" Text="One file for each start of the game."/></StackPanel></DockPanel>
          <DockPanel Margin="0,0,0,12"><Button x:Name="Warm" DockPanel.Dock="Right" Content="Build" MinWidth="110"/><StackPanel><TextBlock Text="Build all pipelines ahead"/><TextBlock Style="{StaticResource Dim}" Text="Optional, about an hour and a half once: nothing is compiled while you play in a new place."/></StackPanel></DockPanel>
          <DockPanel><Button x:Name="Build" DockPanel.Dock="Right" Content="Build" MinWidth="110"/><StackPanel><TextBlock Text="Build the engine"/><TextBlock Style="{StaticResource Dim}" Text="cargo build --release -p vilocity, in a window of its own. After an update of Vilocity."/></StackPanel></DockPanel>
        </StackPanel></Border>
      </StackPanel></ScrollViewer>
      <!-- Log: what the engine wrote in its newest run. -->
      <DockPanel x:Name="Page4" Visibility="Collapsed">
        <TextBlock DockPanel.Dock="Top" Style="{StaticResource Title}" Text="Log"/>
        <TextBlock DockPanel.Dock="Top" x:Name="LogName" Style="{StaticResource Dim}" Margin="0,-8,0,10"/>
        <TextBox x:Name="LogBox" IsReadOnly="True" FontFamily="Consolas" FontSize="12" TextWrapping="NoWrap" VerticalScrollBarVisibility="Auto" HorizontalScrollBarVisibility="Auto" Background="#07080C" BorderBrush="#1F2433"/>
      </DockPanel>
    </Grid>
  </Grid>
</Window>
'@
$w = [Windows.Markup.XamlReader]::Load((New-Object System.Xml.XmlNodeReader $xaml))
$ui = @{}; foreach ($n in 'Logo', 'CoverBox', 'Cover', 'Hero', 'GameName', 'Game', 'Browse', 'Checks', 'Recheck', 'Mode0', 'Mode1', 'Mode2', 'ModeText', 'Cap0', 'Cap1', 'Cap2', 'Sharp', 'SharpText', 'Fsr', 'Hud', 'Full', 'LogWindow', 'Sandbox', 'Dlss', 'DlssText', 'FrameGen', 'Steady', 'Rays', 'Night', 'Smooth', 'Leave', 'Extra', 'Status', 'Keys', 'Saves', 'Logs', 'Build', 'Warm', 'Play', 'Profile', 'LogBox', 'LogName', 'Nav0', 'Nav1', 'Nav2', 'Nav3', 'Nav4', 'Page0', 'Page1', 'Page2', 'Page3', 'Page4') { $ui[$n] = $w.FindName($n) }
# The rail: one page shown at a time. The log's page shows the end of the newest log, read again while it is open.
$showLog = {
    $newest = Get-ChildItem (Join-Path $home_ "logs") -Filter "gta6-*.log" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1
    if (-not $newest) { $ui.LogName.Text = "No log yet: one is written at each start of the game."; return }
    $ui.LogName.Text = $newest.FullName
    try {
        $fs = [IO.File]::Open($newest.FullName, 'Open', 'Read', 'ReadWrite'); $from = [math]::Max(0, $fs.Length - 60000); [void]$fs.Seek($from, 'Begin')
        $text = (New-Object IO.StreamReader($fs)).ReadToEnd(); $fs.Close()
        $ui.LogBox.Text = ($text -split "`r?`n" | Select-Object -Skip 1 | ForEach-Object { $_ -replace '^\S+T(\d\d:\d\d:\d\d)\.\d+Z\s+', '$1 ' }) -join "`r`n"; $ui.LogBox.ScrollToEnd()
    } catch {}
}
foreach ($i in 0..4) {
    $ui["Nav$i"].Tag = $i
    $ui["Nav$i"].Add_Checked({ param($from) foreach ($k in 0..4) { $ui["Page$k"].Visibility = if ($k -eq $from.Tag) { 'Visible' } else { 'Collapsed' } }; if ($from.Tag -eq 4) { & $showLog } })
}
# The logo, in the window, its title bar and its taskbar button.
$logo = Join-Path $root "assets\logo-256.png"
if (Test-Path $logo) { $picture = New-Object Windows.Media.Imaging.BitmapImage ([Uri]$logo); $ui.Logo.Source = $picture; $w.Icon = $picture }

$ui.Game.Text = [string]$set.game
$ui["Mode$([math]::Min([int]$set.mode, 2))"].IsChecked = $true; $ui["Cap$([math]::Min([int]$set.cap, 2))"].IsChecked = $true
$ui.Sharp.Value = [double]$set.sharp; $ui.Fsr.IsChecked = [bool]$set.fsr; $ui.Hud.IsChecked = [bool]$set.hud
$ui.Full.IsChecked = [bool]$set.full; $ui.LogWindow.IsChecked = [bool]$set.logwin; $ui.Rays.IsChecked = [bool]$set.rays; $ui.Steady.IsChecked = [bool]$set.steady; $ui.Dlss.IsChecked = [bool]$set.dlss; $ui.FrameGen.IsChecked = [bool]$set.framegen; $ui.Sandbox.IsChecked = [bool]$set.sandbox; $ui.Night.IsChecked = [bool]$set.night; $ui.Leave.IsChecked = [bool]$set.leave; $ui.Extra.Text = [string]$set.extra
# DLSS: to be chosen only with NVIDIA's Streamline SDK there (the user's own download); it says so otherwise.
$sl = if ($env:VILOCITY_STREAMLINE_DIR) { $env:VILOCITY_STREAMLINE_DIR } else { Join-Path $home_ "streamline" }
if (-not ((Test-Path (Join-Path $sl "sl.interposer.dll")) -or (Test-Path (Join-Path $sl "bin\x64\sl.interposer.dll")))) {
    $ui.Dlss.IsEnabled = $false; $ui.FrameGen.IsEnabled = $false
    $ui.DlssText.Text = "DLSS (NVIDIA RTX cards): needs NVIDIA's Streamline SDK, which is not part of Vilocity. Download it from github.com/NVIDIA-RTX/Streamline/releases and unpack it into $sl"
}
# Frame generation: Streamline refuses it while Windows' GPU scheduling is off ("requires GPU
# hardware scheduling to be enabled in the OS"), and the game then runs with DLSS alone.
if ((Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' -ErrorAction SilentlyContinue).HwSchMode -ne 2) {
    $ui.FrameGen.IsEnabled = $false
    $note = New-Object Windows.Controls.TextBlock; $note.TextWrapping = 'Wrap'
    $note.Text = "Frame generation (DLSS, RTX 40 and 50): needs Windows' hardware-accelerated GPU scheduling, which is off on this PC. Turn it on in Windows Settings, System, Display, Graphics, and restart the PC."
    $ui.FrameGen.Content = $note
}
# Smooth Motion as the driver holds it now for vilocity.exe; no NVIDIA driver: no such option.
$smooth = @{ driver = $null }
try { $smooth.driver = [NvSmooth]::Get('vilocity.exe') -eq 1; $ui.Smooth.IsChecked = $smooth.driver } catch { $ui.Smooth.Visibility = 'Collapsed' }
# 0: 1080p, 2: 1440p, 1: 4K (1 was "Quality", the same picture, in settings kept from before).
function Mode { if ($ui.Mode1.IsChecked) { 1 } elseif ($ui.Mode2.IsChecked) { 2 } else { 0 } }
function Cap { if ($ui.Cap2.IsChecked) { 2 } elseif ($ui.Cap1.IsChecked) { 1 } else { 0 } }
$modeText = { $ui.ModeText.Text = @("The game's Series S picture: 1280x720 upscaled to 1920x1080, the fastest.", "The game's Series X picture for a 4K screen: the most detail, the slowest.", "The game's Series X picture for a 1440p screen: sharper than 1080p, about a quarter slower.")[(Mode)] }
$ui.Mode0.Add_Checked($modeText); $ui.Mode1.Add_Checked($modeText); $ui.Mode2.Add_Checked($modeText); & $modeText
$sharpText = { $ui.SharpText.Text = if ($ui.Sharp.Value -le 0) { "off, as the game asks" } else { "{0:N1}" -f $ui.Sharp.Value } }
$ui.Sharp.Add_ValueChanged($sharpText); & $sharpText

function Save-Choices {
    New-Item -ItemType Directory -Force $home_ | Out-Null
    @{ game = $ui.Game.Text.Trim(); mode = (Mode); cap = (Cap); sharp = [math]::Round($ui.Sharp.Value, 1)
       fsr = [bool]$ui.Fsr.IsChecked; hud = [bool]$ui.Hud.IsChecked; sandbox = [bool]$ui.Sandbox.IsChecked; night = [bool]$ui.Night.IsChecked
       full = [bool]$ui.Full.IsChecked; logwin = [bool]$ui.LogWindow.IsChecked; rays = [bool]$ui.Rays.IsChecked; steady = [bool]$ui.Steady.IsChecked; dlss = [bool]$ui.Dlss.IsChecked; framegen = [bool]$ui.FrameGen.IsChecked; leave = [bool]$ui.Leave.IsChecked; extra = $ui.Extra.Text.Trim() } | ConvertTo-Json | Set-Content $file
}

# One line of the check list: a dot (green: fine, yellow: it works but, red: it will not start) and what was found.
# $do and $action: a small button on the right of the line, and what it does.
function Add-Check([string]$level, [string]$text, [string]$do = "", [scriptblock]$action = $null) {
    $colour = @{ ok = "#3DDC84"; warn = "#FACC15"; bad = "#F87171" }[$level]
    $row = New-Object Windows.Controls.DockPanel; $row.Margin = "0,0,0,9"
    $dot = New-Object Windows.Shapes.Ellipse; $dot.Width = 9; $dot.Height = 9; $dot.Margin = "0,5,10,0"; $dot.VerticalAlignment = "Top"
    $dot.Fill = (New-Object Windows.Media.BrushConverter).ConvertFromString($colour)
    $t = New-Object Windows.Controls.TextBlock; $t.Text = $text; $t.TextWrapping = "Wrap"
    [void]$row.Children.Add($dot)
    if ($action) {
        $b = New-Object Windows.Controls.Button; $b.Content = $do; $b.Margin = "10,0,0,0"; $b.Padding = "10,2"; $b.VerticalAlignment = "Top"
        [Windows.Controls.DockPanel]::SetDock($b, 'Right'); $b.Add_Click($action); [void]$row.Children.Add($b)
    }
    [void]$row.Children.Add($t); [void]$ui.Checks.Children.Add($row)
}

# Looks at the PC and the folders; whether the game can start.
function Test-Machine {
    $ui.Checks.Children.Clear(); $ready = $true
    # What the play script reads of this PC and sets for it (its own first line).
    try {
        $line = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "play-gta6.ps1") -Describe -Game . 2>$null | Select-Object -First 1
        if ($line -match '^This PC: (.+)$') { $ui.Profile.Text = $Matches[1]; $ui.Profile.Visibility = 'Visible' }
    } catch {}
    $game = $ui.Game.Text.Trim().Trim('"')
    $ui.CoverBox.Visibility = 'Collapsed'; $ui.GameName.Text = "Choose your game folder"; $ui.Hero.ImageSource = $null
    if ($game -and (Test-Path (Join-Path $game "game_scarlett_master.exe"))) {
        Add-Check ok "Game folder: game_scarlett_master.exe is there"
        # The game's own name and logo, as its manifest names them (files of your copy).
        $manifest = Get-Content (Join-Path $game "MicrosoftGame.config") -Raw -ErrorAction SilentlyContinue
        if ($manifest -match 'DefaultDisplayName="([^"]+)"') { $ui.GameName.Text = $Matches[1] }
        if ($manifest -match 'SplashScreenImage="([^"]+)"' -and (Test-Path (Join-Path $game $Matches[1]))) {
            $splash = New-Object Windows.Media.Imaging.BitmapImage; $splash.BeginInit(); $splash.CacheOption = 'OnLoad'; $splash.DecodePixelWidth = 1280; $splash.UriSource = [Uri](Join-Path $game $Matches[1]); $splash.EndInit()
            $ui.Hero.ImageSource = $splash
        }
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
            if ($p -and $p.ProcessName -notin 'vilocity', 'dwm') {
                # Closed only by this button, after a question: what is not saved in that program is lost.
                $name = $p.ProcessName; $its = $p.Id; $again = $refresh
                $close = {
                    $sure = [Windows.MessageBox]::Show("Close $name now?`n`nAnything not saved in it is lost.", "Vilocity", 'YesNo', 'Warning')
                    if ($sure -eq 'Yes') { Stop-Process -Id $its -Force -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 900; & $again }
                }.GetNewClosure()
                Add-Check warn ("{0} is holding {1:N1} GB of video memory. Closing it gives the game sharper textures and steadier frames." -f $name, ($m.DedicatedUsage / 1GB)) "Close it" $close
            }
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
$refresh = { $state.ready = Test-Machine; if (-not $state.started) { $ui.Play.IsEnabled = $state.ready; $ui.Status.Text = if ($state.ready) { "Ready." } else { "Not ready yet: see This PC." } } }

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

# Every pipeline the game has made in earlier runs (the engine keeps the list), compiled in a
# window of its own: in the game's sandbox or outside it, as the game will run.
$ui.Warm.Add_Click({
    $book = Join-Path $home_ "cache\pipelines\known-pipelines.bin"
    if (-not (Test-Path $book)) { $ui.Status.Text = "Nothing to build yet: play once first, the game's pipelines are noted while it runs."; return }
    $sure = [Windows.MessageBox]::Show("Build every shader pipeline ahead?`n`nAbout an hour and a half with the processor busy, and about 10 GB on drive C: for the graphics driver's cache. The window can be closed at any time: the next time goes on where it stopped.`n`nNot needed for places you have already played in: those are made ahead at every start.", "Vilocity", 'YesNo', 'Question')
    if ($sure -ne 'Yes') { return }
    $how = if ($ui.Sandbox.IsChecked) { "warm" } else { "native warm" }
    Start-Process cmd -ArgumentList '/k', "`"$exe`" $how && echo. && echo Done. Close this window."
})

$ui.Play.Add_Click({
    if ($state.started) {
        # The play script with all it started (its step, the game, the panel, the log window): ending
        # only vilocity.exe let the script go on to its next step, and the game started all the same.
        if ($state.proc -and -not $state.proc.HasExited) { & taskkill.exe /PID $state.proc.Id /T /F 2>&1 | Out-Null }
        Get-Process vilocity -ErrorAction SilentlyContinue | Stop-Process -Force
        $state.started = $null; $state.proc = $null; $ui.Play.Content = "PLAY"; & $refresh; $ui.Status.Text = "Stopped."
        return
    }
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
    if ($ui.Night.IsChecked) { $vars.VILOCITY_AMBIENT_FLOOR = "0.05" }
    if ($ui.Steady.IsChecked) { $vars.VILOCITY_SUBMIT_DEPTH = "1" }
    if ($ui.Full.IsChecked) { $vars.VILOCITY_FULLSCREEN = "1" }
    foreach ($pair in ($ui.Extra.Text -split ';')) { $k, $v = $pair -split '=', 2; if ($k -and $k.Trim() -match '^VILOCITY_[A-Z0-9_]+$' -and $null -ne $v) { $vars[$k.Trim()] = $v.Trim() } }
    $sets = ($vars.GetEnumerator() | ForEach-Object { "`$env:$($_.Key) = '$($_.Value -replace "'", "''")'; " }) -join ''
    $switches = "-NoMonitor" + " -Picture " + @("1080p", "4k", "1440p")[(Mode)] + $(if (-not $ui.Sandbox.IsChecked) { " -NoSandbox" }) + $(if ($ui.LogWindow.IsChecked) { " -LogWindow" }) + $(if ($ui.Rays.IsChecked) { " -RayTracing" }) + $(if (($ui.Dlss.IsChecked -or $ui.FrameGen.IsChecked) -and $ui.Dlss.IsEnabled) { " -Dlss quality" }) + $(if ($ui.FrameGen.IsChecked -and $ui.Dlss.IsEnabled) { " -FrameGen" })
    $play = Join-Path $PSScriptRoot "play-gta6.ps1"
    $cmd = "$sets & '$($play -replace "'", "''")' -Game '$($ui.Game.Text.Trim().Trim('"') -replace "'", "''")' $switches"
    $state.proc = Start-Process powershell -PassThru -WindowStyle Hidden -ArgumentList "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "`"$($cmd -replace '"', '\"')`""
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
    } elseif (-not $state.seen -and $state.proc -and -not $state.proc.HasExited) {
        # The play script is still at its steps before the game; it is not "did not start" while it runs
        # (said after 45 seconds, PLAY came back and a second press started a second game).
        $step = (Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" | Select-Object -ExpandProperty CommandLine) -join ' '
        $ui.Status.Text = if ($step -like '*native shaders*') { "Translating the game's shaders (once per build, about a minute)..." }
            elseif ($step -like '*warm*') { "Preparing this build's pipelines (once per build, a few minutes)..." }
            else { "Starting..." }
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

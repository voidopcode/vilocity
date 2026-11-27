# The live log of a Vilocity GTA VI run, in a console of its own: every line the engine
# writes, as it writes it, coloured by its kind. It closes with the game.
#   scripts\log-gta6.ps1 [-Log <file>]   (default: the newest log)
param([string]$Log)
$logs = Join-Path $env:LOCALAPPDATA "vilocity\logs"
if (-not $Log) { $Log = Get-ChildItem $logs -Filter *.log -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1 -ExpandProperty FullName }
$host.UI.RawUI.WindowTitle = "Vilocity log"
try { $host.UI.RawUI.BackgroundColor = 'Black'; Clear-Host } catch {}
Write-Host "Vilocity log: $Log" -ForegroundColor DarkGray
$at = 0L; $seen = $false; $gone = 0
while ($true) {
    if ($Log -and (Test-Path $Log)) {
        $fs = [IO.File]::Open($Log, 'Open', 'Read', 'ReadWrite')
        try {
            if ($fs.Length -lt $at) { $at = 0 }
            [void]$fs.Seek($at, 'Begin'); $text = (New-Object IO.StreamReader($fs)).ReadToEnd(); $at = $fs.Length
        } finally { $fs.Close() }
        foreach ($line in ($text -split "`r?`n")) {
            if (-not $line) { continue }
            # "2026-11-27T05:04:19.667140Z  INFO vilocity_gpu::host: what it says"
            if ($line -match '^\S+T(\d\d:\d\d:\d\d)\.\d+Z\s+(\w+)\s+(\S+?):\s(.*)$') {
                $colour = @{ INFO = 'Green'; WARN = 'Yellow'; ERROR = 'Red'; DEBUG = 'DarkGray' }[$Matches[2]]; if (-not $colour) { $colour = 'Gray' }
                Write-Host $Matches[1] -ForegroundColor DarkGray -NoNewline
                Write-Host (" {0,-5} " -f $Matches[2]) -ForegroundColor $colour -NoNewline
                Write-Host ($Matches[3] + ": ") -ForegroundColor DarkCyan -NoNewline
                Write-Host $Matches[4] -ForegroundColor $(if ($Matches[2] -eq 'INFO') { 'White' } else { $colour })
            } else { Write-Host $line -ForegroundColor Gray }
        }
    }
    # With the game: seen once, then gone for three seconds, and the window closes.
    $game = Get-CimInstance Win32_Process -Filter "Name='vilocity.exe'" -ErrorAction SilentlyContinue | Where-Object CommandLine -like '*native run*' | Select-Object -First 1
    if ($game) { $seen = $true; $gone = 0 } elseif ($seen -and ++$gone -ge 6) { break }
    Start-Sleep -Milliseconds 500
}
Write-Host "`nThe game closed. This window closes in 10 seconds." -ForegroundColor DarkGray
Start-Sleep 10

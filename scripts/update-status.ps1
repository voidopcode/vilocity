# Rewrites the "Latest test run" block of README.md from the newest GTA VI log
# in %LOCALAPPDATA%\vilocity\logs. Run by .githooks/pre-commit; safe to run by hand.
# Only numbers go into the README, never paths or log lines.
param([string]$Readme = (Join-Path $PSScriptRoot "..\README.md"))
$ErrorActionPreference = "Stop"

$log = Get-ChildItem (Join-Path $env:LOCALAPPDATA "vilocity\logs") -Filter "gta6-*.log" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $log) { Write-Host "update-status: no GTA VI log, README left as is"; exit 0 }

$fps = @(); $ms = @(); $draws = @(); $dropped = @()
$untranslated = @{}; $refused = @{}; $build = $null
foreach ($line in [IO.File]::ReadLines($log.FullName)) {
    if ($line -match 'vilocity_gpu::host: ([\d.]+) fps, ([\d.]+) ms/frame.*?(\d+) draws/frame, dropped (\d+) without a host pipeline') {
        $fps += [double]$Matches[1]; $ms += [double]$Matches[2]
        $draws += [int]$Matches[3]; $dropped += [int]$Matches[4]
    } elseif ($line -match 'shader does not translate.*stage="(\w+)" hash=(\w+)') {
        $untranslated["$($Matches[1]) $($Matches[2])"] = $Matches[1]
    } elseif ($line -match 'host \w+ pipeline refused.*shaders=\[(.*)\]') {
        $refused[$Matches[1]] = 1
    } elseif (-not $build -and $line -match '^build\s+(Series [\d.]+)') {
        $build = $Matches[1]
    }
}

$inv = [Globalization.CultureInfo]::InvariantCulture
function Fmt($v, $f) { $v.ToString($f, $inv) }
if ($fps.Count) {
    $sorted = $ms | Sort-Object
    $median = $sorted[[int][math]::Floor(($sorted.Count - 1) / 2)]
    $rows = @(
        "| Best frame rate (menus included) | $(Fmt (($fps | Measure-Object -Maximum).Maximum) '0.0') fps |",
        "| Median frame time | $(Fmt $median '0') ms |",
        "| Last frame | $(Fmt $fps[-1] '0.0') fps, $(Fmt $ms[-1] '0') ms, $($draws[-1]) draws, $($dropped[-1]) dropped without a pipeline |"
    )
} else {
    $rows = @("| Frame rate | no world frames measured in this run |")
}
$byStage = $untranslated.Values | Group-Object | Sort-Object Name | ForEach-Object { "$($_.Name) $($_.Count)" }
$rows += "| Shaders that failed to translate | $(if ($byStage) { $byStage -join ', ' } else { 'none' }) |"
$rows += "| Host pipelines refused by the driver | $($refused.Count) |"

$block = @(
    "<!-- status:start (written by scripts/update-status.ps1 on every commit; edits here are overwritten) -->",
    "**Latest test run:** $($log.LastWriteTime.ToString('yyyy-MM-dd HH:mm', $inv))$(if ($build) { ", game build $build" })",
    "",
    "| Measure | Value |",
    "|---------|-------|"
) + $rows + "<!-- status:end -->"

$text = [IO.File]::ReadAllText((Resolve-Path $Readme))
$nl = if ($text.Contains("`r`n")) { "`r`n" } else { "`n" }
$new = ($block -join $nl)
$pattern = '(?s)<!-- status:start.*?<!-- status:end -->'
if ($text -notmatch $pattern) { Write-Error "update-status: README has no status markers" }
$updated = [regex]::Replace($text, $pattern, { param($m) $new })
if ($updated -ne $text) {
    [IO.File]::WriteAllText((Resolve-Path $Readme), $updated, (New-Object Text.UTF8Encoding $false))
    Write-Host "update-status: README updated from $($log.Name)"
}

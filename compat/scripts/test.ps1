# Build everything (the e2e test needs the test title + DLL) and run all tests.
# Arguments go to the test binaries (after `--`), e.g. `.\scripts\test.ps1 --skip <name>`.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
# tests on game data pass without running anything when their variable is not set
$missing = 'VILOCITY_SAMPLE_RPF', 'VILOCITY_SAMPLE_DLLS', 'VILOCITY_SAMPLE_TTPF', 'VILOCITY_SAMPLE_BCPACK', 'VILOCITY_SAMPLE_BK2', 'VILOCITY_SAMPLE_BVH4', 'VILOCITY_SAMPLE_AWC', 'VILOCITY_SAMPLE_XG', 'VILOCITY_SAMPLE_DXC' | Where-Object { -not (Test-Path "env:$_") }
if ($missing) { Write-Host "not set, their tests are skipped: $($missing -join ', ')" }
# Windows PowerShell 5.1 treats redirected native stderr as an error even when Cargo succeeds.
$null = Get-Command cargo -ErrorAction Stop
$ErrorActionPreference = 'Continue'
cargo build --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# GPU tests that cannot run on this adapter pass too: each prints "SKIPPED <test>: <reason>"
# to stderr (VILOCITY_REQUIRE_GPU=wave64 / =mesh makes them fail); counted at the end
$skipped = @()
cargo test --workspace -- --test-threads=4 @args 2>&1 | ForEach-Object {
    $line = "$_"
    if ($line -match '^SKIPPED \S+: (.+)$') { $skipped += $Matches[1] }
    Write-Host $line
}
$code = $LASTEXITCODE
if ($skipped) {
    $counts = $skipped | Group-Object | Sort-Object Count -Descending | ForEach-Object { "$($_.Count) ($($_.Name))" }
    Write-Host "GPU tests skipped on this adapter: $($counts -join ', ')"
}
exit $code

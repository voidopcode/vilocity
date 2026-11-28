# get-fsr.ps1 [-Into <folder>]: AMD's FidelityFX DLLs (the FSR upscaler and FSR frame generation) for Vilocity, once.
# They are not part of Vilocity: each PC gets AMD's signed DLLs from AMD's own FidelityFX SDK on GitHub
# (GPUOpen-LibrariesAndSDKs/FidelityFX-SDK v2.4.0, Kits/FidelityFX/signedbin, MIT licence: 87 MB in all), each
# checked against the git hash GitHub holds for it and against AMD's signature, plus the SDK's licence, in
# %LOCALAPPDATA%\vilocity\fidelityfx, where the play script and the engine look. Exit code 0: ready.
param([string]$Into = (Join-Path $env:LOCALAPPDATA "vilocity\fidelityfx"))
$ErrorActionPreference = 'Stop'
$tag = "v2.4.0"
$base = "https://raw.githubusercontent.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/$tag/Kits/FidelityFX"
# Name, git blob hash (the repository's tree at the tag), size.
$files = @(
    @("signedbin/amd_fidelityfx_loader_dx12.dll", "f1afee09dd3bcded005c722b1e94befb3ee3f9e6", 39184),
    @("signedbin/amd_fidelityfx_upscaler_dx12.dll", "6d01d9f52c412fc336350f04e948eb1bee196a38", 57664272),
    @("signedbin/amd_fidelityfx_framegeneration_dx12.dll", "445674331676665f18b5acd1b3c8621629cb0e75", 29679376),
    @("docs/license.md", "aef7adddbcf121c1f7f1248fe97de0f7f7a3d719", 65770)
)
function Get-GitBlobSha([byte[]]$b) {
    $head = [Text.Encoding]::ASCII.GetBytes("blob $($b.Length)`0")
    $sha = [Security.Cryptography.SHA1]::Create()
    [void]$sha.TransformBlock($head, 0, $head.Length, $null, 0); [void]$sha.TransformFinalBlock($b, 0, $b.Length)
    -join ($sha.Hash | ForEach-Object { $_.ToString('x2') })
}
New-Item -ItemType Directory -Force $Into | Out-Null
$curl = Join-Path $env:SystemRoot "System32\curl.exe"
try {
    foreach ($f in $files) {
        $name = Split-Path $f[0] -Leaf; $to = Join-Path $Into $(if ($name -eq 'license.md') { 'FidelityFX-license.md' } else { $name })
        if ((Test-Path $to) -and (Get-GitBlobSha ([IO.File]::ReadAllBytes($to))) -eq $f[1]) { Write-Host "$name is already there."; continue }
        Write-Host ("Getting {0} ({1:N1} MB, from AMD's FidelityFX SDK {2} on GitHub)..." -f $name, ($f[2] / 1MB), $tag)
        $tmp = "$to.download"
        if (Test-Path $curl) { & $curl -L --fail --retry 2 -o $tmp "$base/$($f[0])"; if ($LASTEXITCODE -ne 0) { throw "the download of $name stopped (curl $LASTEXITCODE)" } }
        else { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; (New-Object Net.WebClient).DownloadFile("$base/$($f[0])", $tmp) }
        if ((Get-GitBlobSha ([IO.File]::ReadAllBytes($tmp))) -ne $f[1]) { Remove-Item $tmp; throw "$name is not AMD's file (its hash differs)" }
        if ($name -like '*.dll') {
            $signed = Get-AuthenticodeSignature $tmp
            if ($signed.Status -ne 'Valid' -or $signed.SignerCertificate.Subject -notlike 'CN=Advanced Micro Devices*') { Remove-Item $tmp; throw "$name is not signed by AMD" }
        }
        Move-Item $tmp $to -Force
    }
    Write-Host "AMD FSR is ready: $Into (AMD's licence beside it). FSR frame generation can be turned on in the launcher's Settings or in the game (F1)." -ForegroundColor Green
    exit 0
} catch {
    Write-Host "Could not get AMD FSR: $($_.Exception.Message)" -ForegroundColor Yellow
    exit 1
}

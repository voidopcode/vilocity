# get-dlss.ps1 [-Into <folder>]: NVIDIA's Streamline SDK (DLSS, frame generation) for Vilocity, once.
# It is not part of Vilocity (NVIDIA's licence): each PC gets it from NVIDIA's own release on GitHub
# (NVIDIA-RTX/Streamline v2.14.16, the version the engine is built against, 276 MB), checked against the
# SHA-256 GitHub lists for it, and only bin\x64 (the DLLs and their licences) and the SDK's licence files
# are kept, in %LOCALAPPDATA%\vilocity\streamline, where the launcher and the engine look. The DLLs must
# be signed by NVIDIA, or they are removed again. Exit code 0: ready.
param([string]$Into = $(if ($env:VILOCITY_STREAMLINE_DIR) { $env:VILOCITY_STREAMLINE_DIR } else { Join-Path $env:LOCALAPPDATA "vilocity\streamline" }))
$ErrorActionPreference = 'Stop'
$ver = "2.14.16"
$url = "https://github.com/NVIDIA-RTX/Streamline/releases/download/v$ver/streamline-sdk-v$ver.zip"
$sha256 = "32041e51db0cab709a271b72a352281b690c2abbd669a827809401a5dd23956b"
$bin = Join-Path $Into "bin\x64"
if (Test-Path (Join-Path $bin "sl.interposer.dll")) { Write-Host "DLSS is already there: $bin"; exit 0 }
$tmp = Join-Path $env:TEMP "vilocity-streamline-$ver.zip"
try {
    Write-Host "Getting NVIDIA's Streamline SDK $ver (276 MB, from github.com/NVIDIA-RTX/Streamline)..."
    # (Windows' own curl.exe: fast, with its progress; Invoke-WebRequest's crawls in PowerShell 5.)
    $curl = Join-Path $env:SystemRoot "System32\curl.exe"
    if (Test-Path $curl) {
        & $curl -L --fail --retry 2 -o $tmp $url
        if ($LASTEXITCODE -ne 0) { throw "the download stopped (curl $LASTEXITCODE)" }
    } else {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        (New-Object Net.WebClient).DownloadFile($url, $tmp)
    }
    $got = (Get-FileHash -Algorithm SHA256 $tmp).Hash.ToLowerInvariant()
    if ($got -ne $sha256) { throw "the download is not NVIDIA's release (its SHA-256 differs)" }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    New-Item -ItemType Directory -Force $bin | Out-Null
    $zip = [IO.Compression.ZipFile]::OpenRead($tmp)
    try {
        foreach ($e in $zip.Entries) {
            $path = $e.FullName -replace '\\', '/'
            if (-not $e.Name) { continue }
            # bin/x64's own files (not its debug "development" folder), and the SDK's licences.
            if ($path -match '(^|/)bin/x64/[^/]+$') { $to = Join-Path $bin $e.Name }
            elseif ($path -notmatch '/' -and $e.Name -match 'licen[cs]e|\.md$') { $to = Join-Path $Into $e.Name }
            else { continue }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($e, $to, $true)
        }
    } finally { $zip.Dispose() }
    foreach ($dll in "sl.interposer.dll", "nvngx_dlss.dll") {
        $signed = Get-AuthenticodeSignature (Join-Path $bin $dll)
        if ($signed.Status -ne 'Valid' -or $signed.SignerCertificate.Subject -notlike 'CN=NVIDIA Corporation*') {
            Remove-Item $bin -Recurse -Force -ErrorAction SilentlyContinue
            throw "$dll is not signed by NVIDIA"
        }
    }
    Write-Host "DLSS is ready: $bin (NVIDIA's licences beside it). Choose an upscaler in the launcher's Settings or in the game (F1)." -ForegroundColor Green
    exit 0
} catch {
    Write-Host "Could not get DLSS: $($_.Exception.Message)" -ForegroundColor Yellow
    exit 1
} finally { Remove-Item $tmp -ErrorAction SilentlyContinue }

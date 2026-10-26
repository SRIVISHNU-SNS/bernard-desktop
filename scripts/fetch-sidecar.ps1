<#
.SYNOPSIS
    Fetch the pinned llama.cpp server binaries that Bernard ships with.

.DESCRIPTION
    Downloads the two Windows x64 llama.cpp release archives that Bernard supports,
    verifies their SHA-256 digests, and extracts only the files the inference server
    needs into sidecar/llama-vulkan and sidecar/llama-cpu.

    The sidecar folders are git-ignored: run this script once after cloning and again
    on any machine that builds the installer.

.PARAMETER Force
    Re-download the archives even if a verified copy already exists in the cache.

.PARAMETER CacheDir
    Where downloaded archives are kept. Defaults to %LOCALAPPDATA%\bernard-sidecar-cache.
#>
[CmdletBinding()]
param(
    [switch]$Force,
    [string]$CacheDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$release = 'b11889'
$root = Split-Path $PSScriptRoot -Parent
$destinationRoot = Join-Path $root 'sidecar'

if (-not $CacheDir) {
    $CacheDir = Join-Path $env:LOCALAPPDATA 'bernard-sidecar-cache'
}

New-Item -ItemType Directory -Force -Path $CacheDir | Out-Null

$targets = @(
    @{
        Name = 'llama-vulkan'
        Asset = "llama-$release-bin-win-vulkan-x64.zip"
        Sha256 = '3874D6CB33C3540A68191C1CA31B774BD72914E8CA65E51DFE23E3DAE8201F00'
        Size = 37049027
    },
    @{
        Name = 'llama-cpu'
        Asset = "llama-$release-bin-win-cpu-x64.zip"
        Sha256 = 'BF563DFFB502A230AE06C9DC455B54CBC079E7E0BC270CD4D6191209B28D175D'
        Size = 24226875
    }
)

# Files llama-server needs at runtime. Everything else in the release archive (extra
# CLIs, benchmarks, RPC helpers) is left behind to keep the installer small.
$runtimePatterns = @(
    'llama-server.exe',
    'llama-server-impl.dll',
    'llama.dll',
    'llama-common.dll',
    'mtmd.dll',
    'ggml.dll',
    'ggml-base.dll',
    'ggml-cpu-*.dll',
    'ggml-vulkan.dll',
    'ggml-rpc.dll',
    'libomp.dll',
    'LICENSE-LLVM-OpenMP'
)

function Get-VerifiedArchive {
    param(
        [hashtable]$Target,
        [string]$CacheDir,
        [switch]$Force
    )

    $path = Join-Path $CacheDir $Target.Asset

    if ((Test-Path $path) -and -not $Force) {
        $existing = (Get-FileHash -Path $path -Algorithm SHA256).Hash
        if ($existing -eq $Target.Sha256) {
            Write-Host "  cached  $($Target.Asset)"
            return $path
        }
        Write-Host "  stale   $($Target.Asset) (hash mismatch, re-downloading)"
    }

    $url = "https://github.com/ggml-org/llama.cpp/releases/download/$release/$($Target.Asset)"
    Write-Host "  fetch   $($Target.Asset) ($([math]::Round($Target.Size / 1MB, 1)) MB)"

    $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
    if ($curl) {
        # curl handles slow/aborted transfers better than Invoke-WebRequest.
        & $curl.Source --fail --location --retry 10 --retry-all-errors --retry-delay 3 `
            --continue-at - --output $path $url
        if ($LASTEXITCODE -ne 0) {
            throw "Download failed for $($Target.Asset) (curl exit $LASTEXITCODE)."
        }
    }
    else {
        Invoke-WebRequest -Uri $url -OutFile $path -UseBasicParsing
    }

    $actual = (Get-FileHash -Path $path -Algorithm SHA256).Hash
    if ($actual -ne $Target.Sha256) {
        Remove-Item -Force $path -ErrorAction SilentlyContinue
        throw "Checksum mismatch for $($Target.Asset). Expected $($Target.Sha256), got $actual."
    }

    return $path
}

foreach ($target in $targets) {
    Write-Host "Sidecar: $($target.Name)"
    $archive = Get-VerifiedArchive -Target $target -CacheDir $CacheDir -Force:$Force

    $destination = Join-Path $destinationRoot $target.Name
    if (Test-Path $destination) {
        Remove-Item -Recurse -Force $destination
    }
    New-Item -ItemType Directory -Force -Path $destination | Out-Null

    $staging = Join-Path $CacheDir ("extract-" + $target.Name)
    if (Test-Path $staging) {
        Remove-Item -Recurse -Force $staging
    }
    New-Item -ItemType Directory -Force -Path $staging | Out-Null
    Expand-Archive -Path $archive -DestinationPath $staging -Force

    $copied = 0
    foreach ($pattern in $runtimePatterns) {
        Get-ChildItem -Path $staging -Filter $pattern -File -Recurse |
            Where-Object { $_.DirectoryName -eq $staging } |
            ForEach-Object {
                Copy-Item -Path $_.FullName -Destination $destination -Force
                $copied++
            }
    }

    Remove-Item -Recurse -Force $staging

    if (-not (Test-Path (Join-Path $destination 'llama-server.exe'))) {
        throw "llama-server.exe missing after extracting $($target.Asset)."
    }

    $bytes = (Get-ChildItem -Path $destination -File | Measure-Object -Property Length -Sum).Sum
    Write-Host ("  ready   sidecar/{0} ({1} files, {2} MB)" -f $target.Name, $copied, [math]::Round($bytes / 1MB, 1))
}

Write-Host 'Sidecars ready.'

<#
.SYNOPSIS
    Download a Gemma 4 preset for Bernard without opening the app.

.DESCRIPTION
    Mirrors what the in-app downloader does: resumable transfer to <file>.part,
    SHA-256 verification, then an atomic rename to the managed model folder.
    Useful for pre-seeding %APPDATA%\com.bernard.desktop\models on a slow link.

.PARAMETER Preset
    e2b, e4b (default) or 12b.

.PARAMETER Destination
    Model folder. Defaults to the app's roaming data folder.

.PARAMETER SkipVerify
    Skip the SHA-256 check (not recommended).
#>
[CmdletBinding()]
param(
    [ValidateSet('e2b', 'e4b', '12b')]
    [string]$Preset = 'e4b',
    [string]$Destination,
    [switch]$SkipVerify
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$presets = @{
    e2b = @{
        Repo         = 'ggml-org/gemma-4-E2B-it-GGUF'
        Model        = 'gemma-4-E2B-it-Q4_0.gguf'
        ModelBytes   = 2841481184
        ModelSha256  = '8e30dff3ac4c8434c49a7036fa15564bdbb6044e42bf04550bf1a096ad7e6a52'
        Mmproj       = 'mmproj-gemma-4-E2B-it-Q8_0.gguf'
        MmprojBytes  = 557368064
        MmprojSha256 = '9406f99c16d68cda4f1f0552192dcc99021ea1fc6d2fd50b1dc3ccf30d04b292'
    }
    e4b = @{
        Repo         = 'ggml-org/gemma-4-E4B-it-GGUF'
        Model        = 'gemma-4-E4B-it-Q4_0.gguf'
        ModelBytes   = 4590807392
        ModelSha256  = 'a555b900214b477d8880e7832e0b8925e139b0159640036b09fe472b6f2097f2'
        Mmproj       = 'mmproj-gemma-4-E4B-it-Q8_0.gguf'
        MmprojBytes  = 559874816
        MmprojSha256 = '197f49a93027f9843772bd24a6a9e0be2a32a788de5a3def330e9c585d86edd1'
    }
    '12b' = @{
        Repo         = 'ggml-org/gemma-4-12B-it-GGUF'
        Model        = 'gemma-4-12B-it-Q4_0.gguf'
        ModelBytes   = 7219673216
        ModelSha256  = '3712b9bd32cae83a22f67ee7a4466d8d7a4f21646ac8a07d19bf9418e8767a70'
        Mmproj       = 'mmproj-gemma-4-12B-it-Q8_0.gguf'
        MmprojBytes  = 158987616
        MmprojSha256 = '59e62255435dda870e2d1de97cc031330b31a898bac12b38a182cecff9cd3738'
    }
}

$spec = $presets[$Preset]
if (-not $Destination) {
    $Destination = Join-Path $env:APPDATA 'com.bernard.desktop\models'
}
New-Item -ItemType Directory -Force -Path $Destination | Out-Null

$curl = (Get-Command curl.exe -ErrorAction SilentlyContinue).Source
if (-not $curl) {
    throw 'curl.exe is required.'
}

function Receive-File {
    param([string]$Url, [string]$Path, [long]$ExpectedBytes)

    $part = "$Path.part"
    if ((Test-Path $Path) -and ((Get-Item $Path).Length -eq $ExpectedBytes)) {
        Write-Host "  present $(Split-Path $Path -Leaf)"
        return
    }

    for ($attempt = 1; $attempt -le 200; $attempt++) {
        $have = if (Test-Path $part) { (Get-Item $part).Length } else { 0 }
        if ($have -ge $ExpectedBytes) { break }
        if ($attempt -gt 1) {
            Write-Host ("  resume  {0:P1} ({1} of {2} bytes)" -f ($have / $ExpectedBytes), $have, $ExpectedBytes)
        }
        & $curl -sS -L --fail --continue-at - --retry 5 --retry-all-errors --retry-delay 3 `
            --speed-limit 2048 --speed-time 90 --output $part $Url
        if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne 28) {
            Start-Sleep -Seconds 3
        }
        $have = if (Test-Path $part) { (Get-Item $part).Length } else { 0 }
        if ($have -ge $ExpectedBytes) { break }
        Start-Sleep -Seconds 2
    }

    $final = if (Test-Path $part) { (Get-Item $part).Length } else { 0 }
    if ($final -ne $ExpectedBytes) {
        throw "Transfer incomplete for $(Split-Path $Path -Leaf): $final of $ExpectedBytes bytes."
    }
    Move-Item -Force $part $Path
    Write-Host "  saved   $(Split-Path $Path -Leaf)"
}

$modelPath = Join-Path $Destination $spec.Model
$mmprojPath = Join-Path $Destination $spec.Mmproj

Write-Host "Preset $Preset -> $Destination"
Receive-File -Url "https://huggingface.co/$($spec.Repo)/resolve/main/$($spec.Model)?download=true" `
    -Path $modelPath -ExpectedBytes $spec.ModelBytes
Receive-File -Url "https://huggingface.co/$($spec.Repo)/resolve/main/$($spec.Mmproj)?download=true" `
    -Path $mmprojPath -ExpectedBytes $spec.MmprojBytes

if (-not $SkipVerify) {
    Write-Host 'Verifying SHA-256 (this reads several GB)...'
    foreach ($pair in @(
            @{ Path = $modelPath; Hash = $spec.ModelSha256 },
            @{ Path = $mmprojPath; Hash = $spec.MmprojSha256 })) {
        $actual = (Get-FileHash -Path $pair.Path -Algorithm SHA256).Hash
        if ($actual -ne $pair.Hash) {
            throw "Checksum mismatch for $(Split-Path $pair.Path -Leaf): $actual"
        }
        Write-Host "  ok      $(Split-Path $pair.Path -Leaf)"
    }
}

Write-Host 'Model ready.'

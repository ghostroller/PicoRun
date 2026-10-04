# Build an explicit Windows x64 payload, ZIP and optional Inno Setup installer.
# Requires Windows PowerShell 5.1+ or PowerShell 7, Rust MSVC and Inno Setup 6.7.3+.
[CmdletBinding()]
param(
    [string]$Iscc,
    [string]$OutputDirectory = 'dist',
    [switch]$ZipOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$workspacePath = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot))
$workspacePrefix = $workspacePath + [IO.Path]::DirectorySeparatorChar
if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    $outputPath = [IO.Path]::GetFullPath($OutputDirectory)
} else {
    $outputPath = [IO.Path]::GetFullPath((Join-Path $workspacePath $OutputDirectory))
}
if (-not $outputPath.StartsWith($workspacePrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'OutputDirectory must be inside the workspace.'
}

if (-not $ZipOnly) {
    if (-not $Iscc) { $Iscc = $env:INNO_SETUP_ISCC }
    if (-not $Iscc) {
        $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
        if ($command) { $Iscc = $command.Source }
    }
    if (-not $Iscc) {
        $candidates = @(
            (Join-Path $workspacePath 'runtime/build-tools/inno-6.7.3/ISCC.exe'),
            "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
            "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
            "$env:ProgramFiles\Inno Setup 7\ISCC.exe"
        )
        foreach ($candidate in $candidates) {
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { $Iscc = $candidate; break }
        }
    }
    if (-not $Iscc -or -not (Test-Path -LiteralPath $Iscc -PathType Leaf)) {
        throw 'Inno Setup compiler not found. Pass -Iscc <ISCC.exe>, set INNO_SETUP_ISCC, or use -ZipOnly.'
    }
    $Iscc = (Resolve-Path -LiteralPath $Iscc).Path
}

Push-Location -LiteralPath $workspacePath
try {
    $metadataJson = & cargo metadata --offline --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    $metadata = ($metadataJson -join "`n") | ConvertFrom-Json
    $package = @($metadata.packages | Where-Object { $_.name -eq 'picorun' })
    if ($package.Count -ne 1) { throw 'Expected one PicoRun package.' }
    $version = $package[0].version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Packaging requires a numeric major.minor.patch version in Cargo.toml.' }
    foreach ($component in $version.Split('.')) {
        if ([uint64]$component -gt 65535) { throw 'Version components must fit Windows version metadata.' }
    }
    & cargo build --release --offline --bin picorun --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Windows x64 release build failed.' }
    $exePath = Join-Path $workspacePath 'target/x86_64-pc-windows-msvc/release/picorun.exe'
    $binary = [IO.File]::ReadAllBytes($exePath)
    $peOffset = [BitConverter]::ToInt32($binary, 0x3c)
    if ($peOffset -lt 0 -or $peOffset + 6 -gt $binary.Length -or
        [BitConverter]::ToUInt32($binary, $peOffset) -ne 0x4550 -or
        [BitConverter]::ToUInt16($binary, $peOffset + 4) -ne 0x8664) {
        throw 'Payload is not an x64 Windows PE executable.'
    }

    $stagePath = Join-Path $workspacePath ('runtime/package-build/' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $stagePath,$outputPath -Force | Out-Null
    $payloadPath = Join-Path $stagePath 'payload'
    New-Item -ItemType Directory -Path $payloadPath -Force | Out-Null
    $payloadFiles = @(
        'picorun.exe', 'README.txt', 'THIRD_PARTY_NOTICES.md',
        'third_party/pinyin-data/LICENSE', 'third_party/inno-setup/LICENSE'
    )
    Copy-Item -LiteralPath $exePath -Destination (Join-Path $payloadPath 'picorun.exe')
    $readme = [IO.File]::ReadAllText((Join-Path $workspacePath 'packaging/windows/README.txt.in')).Replace('@VERSION@', $version)
    [IO.File]::WriteAllText((Join-Path $payloadPath 'README.txt'), $readme, [Text.UTF8Encoding]::new($true))
    foreach ($relativePath in $payloadFiles | Select-Object -Skip 2) {
        $destination = Join-Path $payloadPath $relativePath
        New-Item -ItemType Directory -Path (Split-Path $destination) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $workspacePath $relativePath) -Destination $destination
    }

    Add-Type -AssemblyName System.IO.Compression
    $zipName = "PicoRun-$version-windows-x64.zip"
    $zipPath = Join-Path $outputPath $zipName
    $temporaryZip = Join-Path $stagePath $zipName
    $zipStream = [IO.File]::Open($temporaryZip, [IO.FileMode]::CreateNew)
    $archive = [IO.Compression.ZipArchive]::new($zipStream, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($relativePath in $payloadFiles) {
            $entry = $archive.CreateEntry($relativePath.Replace('\','/'), [IO.Compression.CompressionLevel]::Optimal)
            # Fixed metadata makes identical payloads produce identical ZIP bytes.
            $entry.LastWriteTime = [DateTimeOffset]::new(2000,1,1,0,0,0,[TimeSpan]::Zero)
            $entryStream = $entry.Open()
            $inputStream = [IO.File]::OpenRead((Join-Path $payloadPath $relativePath))
            try { $inputStream.CopyTo($entryStream) } finally { $inputStream.Dispose(); $entryStream.Dispose() }
        }
    } finally { $archive.Dispose(); $zipStream.Dispose() }

    $artifacts = @($zipPath)
    if (-not $ZipOnly) {
        & $Iscc /Qp "/DAppVersion=$version" "/DPayloadDir=$payloadPath" "/DOutputDir=$stagePath" (Join-Path $workspacePath 'packaging/windows/PicoRun.iss')
        if ($LASTEXITCODE -ne 0) { throw 'Inno Setup compilation failed.' }
        $setupName = "PicoRun-$version-windows-x64-setup.exe"
        $temporarySetup = Join-Path $stagePath $setupName
        if (-not (Test-Path -LiteralPath $temporarySetup -PathType Leaf)) { throw 'Installer output missing.' }
        $setupPath = Join-Path $outputPath $setupName
        Copy-Item -LiteralPath $temporarySetup -Destination $setupPath -Force
        $artifacts += $setupPath
    }
    Copy-Item -LiteralPath $temporaryZip -Destination $zipPath -Force
    $sums = foreach ($artifact in $artifacts) {
        $hash = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
        $hash + '  ' + [IO.Path]::GetFileName($artifact)
    }
    [IO.File]::WriteAllLines((Join-Path $outputPath "PicoRun-$version-SHA256SUMS.txt"), [string[]]$sums, [Text.UTF8Encoding]::new($false))
    $details = [ordered]@{
        version = $version
        architecture = 'x64'
        target = 'x86_64-pc-windows-msvc'
        executable_sha256 = (Get-FileHash -LiteralPath $exePath -Algorithm SHA256).Hash
        executable_bytes = (Get-Item -LiteralPath $exePath).Length
        payload_files = $payloadFiles
        rustc = (& rustc --version) -join ''
        powershell_version = $PSVersionTable.PSVersion.ToString()
        dotnet_version = [Environment]::Version.ToString()
        inno_compiler_sha256 = if (-not $ZipOnly) { (Get-FileHash -LiteralPath $Iscc -Algorithm SHA256).Hash } else { $null }
        installer_built = -not [bool]$ZipOnly
    }
    [IO.File]::WriteAllText((Join-Path $outputPath "PicoRun-$version-build.json"), ($details | ConvertTo-Json -Depth 3), [Text.UTF8Encoding]::new($false))
    foreach ($artifact in $artifacts) {
        Get-Item -LiteralPath $artifact | Select-Object Name,Length,FullName
    }
    Write-Host "Payload retained for inspection: $payloadPath"
} finally { Pop-Location }

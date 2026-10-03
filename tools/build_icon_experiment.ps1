# Reconstruct an isolated experiment from the committed baseline. Does not edit product source.
param([string]$Output = 'runtime/icon-eval/prototype-rebuilt', [switch]$Optimized)
$ErrorActionPreference = 'Stop'
$workspacePath = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot))
if ($Optimized -and -not $PSBoundParameters.ContainsKey('Output')) { $Output = 'runtime/icon-eval/optimized-rebuilt' }
$outputPath = [IO.Path]::GetFullPath((Join-Path $workspacePath $Output))
if (-not $outputPath.StartsWith($workspacePath + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'output must be inside workspace' }
if (Test-Path -LiteralPath $outputPath) { throw 'choose a new empty output directory to preserve previous experiments' }
New-Item -ItemType Directory -Path $outputPath | Out-Null
$relativeOutput = [IO.Path]::GetRelativePath($workspacePath,$outputPath).Replace('\','/')
$archivePath = Join-Path $outputPath 'baseline.tar'
Push-Location -LiteralPath $workspacePath
try {
 & git -c "safe.directory=$($workspacePath.Replace('\','/'))" archive --format=tar -o $archivePath 1a0080f
 if ($LASTEXITCODE -ne 0) { throw 'committed baseline export failed' }
 & tar -xf $archivePath -C $outputPath
 if ($LASTEXITCODE -ne 0) { throw 'baseline extraction failed' }
 & git -c "safe.directory=$($workspacePath.Replace('\','/'))" apply --unidiff-zero "--directory=$relativeOutput" tools/icon_experiment.patch
 if ($LASTEXITCODE -ne 0) { throw 'experiment patch application failed' }
 if ($Optimized) {
  & git -c "safe.directory=$($workspacePath.Replace('\','/'))" apply --unidiff-zero "--directory=$relativeOutput" tools/icon_cache_optimization.patch
  if ($LASTEXITCODE -ne 0) { throw 'cache optimization patch application failed' }
 }
} finally { Pop-Location }
Push-Location -LiteralPath $outputPath
try {
 & cargo fmt --check
 if ($LASTEXITCODE -ne 0) { throw 'format check failed' }
 & cargo test --offline
 if ($LASTEXITCODE -ne 0) { throw 'tests failed' }
 & cargo clippy --offline --all-targets -- -D warnings
 if ($LASTEXITCODE -ne 0) { throw 'clippy failed' }
 & cargo build --release --offline
 if ($LASTEXITCODE -ne 0) { throw 'release build failed' }
 Get-FileHash -LiteralPath target/release/picorun.exe -Algorithm SHA256
} finally { Pop-Location }
"Isolated experiment: $outputPath"

# Generate 500 controlled application links, including an explicit icon-resource override.
param([string]$Prototype = 'runtime/icon-eval/prototype')
$ErrorActionPreference = 'Stop'
$workspacePath = Split-Path $PSScriptRoot
$target = Join-Path (Join-Path $workspacePath $Prototype) 'target/release/native_probe.exe'
if (-not (Test-Path -LiteralPath $target)) { throw 'build icon experiment first' }
$sourcePath = Join-Path $workspacePath 'runtime/icon-eval/source'
$workingPath = Join-Path $workspacePath 'runtime/icon-eval/controlled-working'
New-Item -ItemType Directory -Path $sourcePath,$workingPath -Force | Out-Null
$markerPath = Join-Path $workingPath 'child.txt'
$shell = New-Object -ComObject WScript.Shell
try {
 for ($index=0; $index -lt 500; $index++) {
  $name = switch ($index) {
   0 { [string][char]0x5FAE + [char]0x4FE1 }
   1 { [string][char]0x8BB0 + [char]0x4E8B + [char]0x672C }
   2 { [string][char]0x91CD + [char]0x5E86 + [char]0x94F6 + [char]0x884C }
   3 { [string][char]0x817E + [char]0x8BAF + 'QQ' }
   default { 'Synthetic App {0:D4}' -f $index }
  }
  $link = $shell.CreateShortcut((Join-Path $sourcePath "$name.lnk"))
  try {
   $link.TargetPath = $target
   $link.Arguments = '--controlled-child "' + $markerPath + '" icon-evaluation'
   $link.WorkingDirectory = $workingPath
   if ($index -eq 4) { $link.IconLocation = '%SystemRoot%\System32\shell32.dll,-154' }
   $link.Save()
  } finally { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($link) | Out-Null }
 }
} finally { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) | Out-Null }
'Created 500 controlled shortcuts; no applications launched'

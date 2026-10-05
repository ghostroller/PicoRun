# Runs the production ownership parser without installing, then checks startup-failure
# cleanup with a private test executable. No Run values or user launcher are changed.
[CmdletBinding()]
param([string]$Iscc)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$workspacePath = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot))
if (-not $Iscc) { $Iscc = Join-Path $workspacePath 'runtime/build-tools/inno-6.7.3/ISCC.exe' }
$Iscc = (Resolve-Path -LiteralPath $Iscc).Path
$token = [guid]::NewGuid().ToString('N')
$testPath = Join-Path $workspacePath "runtime/probe-packaging-edge/$token"
New-Item -ItemType Directory -Path $testPath -Force | Out-Null
$checks = [Collections.Generic.List[string]]::new()
function Check([bool]$Passed, [string]$Description) {
    if (-not $Passed) { throw "FAIL $Description" }
    $checks.Add("PASS $Description")
    Write-Host "PASS $Description"
}

# Compile the exact Pascal functions from the production installer, without its
# registry cleanup or any installation actions. InitializeSetup declines to install.
$installerSource = [IO.File]::ReadAllText((Join-Path $workspacePath 'packaging/windows/PicoRun.iss'))
$parserStart = $installerSource.IndexOf('function CommandExecutable(')
$parserEnd = $installerSource.IndexOf('procedure RemoveOwnedStartupRegistration;')
if ($parserStart -lt 0 -or $parserEnd -le $parserStart) { throw 'Installer ownership functions not found.' }
$ownershipFunctions = $installerSource.Substring($parserStart, $parserEnd - $parserStart)
$harnessSource = @'
[Setup]
AppId=PicoRun.OwnershipProbe.@TOKEN@
AppName=PicoRun Ownership Probe
AppVersion=1.0.0
PrivilegesRequired=lowest
CreateAppDir=no
Uninstallable=no
OutputDir=@OUTPUT@
OutputBaseFilename=ownership-probe
[Code]
@FUNCTIONS@
procedure ExpectOwnership(Command, Installed: String; Expected: Boolean; LabelText: String);
begin
  if OwnsStartupCommand(Command, Installed) <> Expected then
    RaiseException('Ownership mismatch: ' + LabelText);
  Log('OWNERSHIP_PASS ' + LabelText);
end;
function InitializeSetup(): Boolean;
var
  LocalExe, UncExe: String;
begin
  LocalExe := 'C:\Pico Run\picorun.exe';
  UncExe := '\\server\共享\安装 目录\picorun.exe';
  ExpectOwnership('"' + LocalExe + '" --hidden', LocalExe, True, 'quoted local');
  ExpectOwnership('C:\PicoRun\picorun.exe --hidden', 'C:\PicoRun\picorun.exe', True, 'unquoted local');
  ExpectOwnership('"' + UncExe + '" --hidden', UncExe, True, 'quoted Unicode UNC');
  ExpectOwnership('  "\\SERVER\共享\安装 目录\PICORUN.EXE" --hidden', UncExe, True, 'UNC case and whitespace');
  ExpectOwnership('\\server\share\PicoRun\picorun.exe --hidden', '\\server\share\PicoRun\picorun.exe', True, 'unquoted UNC');
  ExpectOwnership('"\\server\共享\安装 目录\child\..\picorun.exe" --hidden', UncExe, True, 'normalized UNC');
  ExpectOwnership('"\\other\共享\安装 目录\picorun.exe" --hidden', UncExe, False, 'other UNC server');
  ExpectOwnership('"\\server\other\安装 目录\picorun.exe" --hidden', UncExe, False, 'other UNC share');
  ExpectOwnership('"' + UncExe + '.backup" --hidden', UncExe, False, 'UNC prefix collision');
  ExpectOwnership('"' + UncExe + '"suffix --hidden', UncExe, False, 'UNC quote suffix');
  ExpectOwnership('"' + UncExe + ' --hidden', UncExe, False, 'unclosed UNC quote');
  ExpectOwnership('"\\server" --hidden', UncExe, False, 'missing UNC share');
  ExpectOwnership('"\\server\共享" --hidden', UncExe, False, 'missing UNC executable');
  ExpectOwnership('"\\server\\picorun.exe" --hidden', UncExe, False, 'empty UNC share');
  ExpectOwnership('"\\\共享\picorun.exe" --hidden', UncExe, False, 'empty UNC server');
  ExpectOwnership('"\\.\共享\安装 目录\picorun.exe" --hidden', UncExe, False, 'device namespace');
  ExpectOwnership('"\\?\UNC\server\共享\安装 目录\picorun.exe" --hidden', UncExe, False, 'extended namespace');
  ExpectOwnership('"\Pico Run\picorun.exe" --hidden', LocalExe, False, 'root relative');
  ExpectOwnership('"C:Pico Run\picorun.exe" --hidden', LocalExe, False, 'drive relative');
  ExpectOwnership('"Pico Run\picorun.exe" --hidden', LocalExe, False, 'relative');
  ExpectOwnership('"1:\Pico Run\picorun.exe" --hidden', '1:\Pico Run\picorun.exe', False, 'invalid drive');
  Log('OWNERSHIP_PROBE_PASSED');
  Result := False;
end;
'@
$harnessSource = $harnessSource.Replace('@TOKEN@', $token).Replace('@OUTPUT@', $testPath).Replace('@FUNCTIONS@', $ownershipFunctions)
$harnessPath = Join-Path $testPath 'ownership-probe.iss'
[IO.File]::WriteAllText($harnessPath, $harnessSource, [Text.UTF8Encoding]::new($true))
& $Iscc /Q $harnessPath
if ($LASTEXITCODE -ne 0) { throw 'Ownership probe compilation failed.' }
$logPath = Join-Path $testPath 'ownership.log'
$harness = Start-Process -FilePath (Join-Path $testPath 'ownership-probe.exe') -ArgumentList @(
    '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', ('/LOG="' + $logPath + '"')
) -WindowStyle Hidden -Wait -PassThru
try {
    $log = [IO.File]::ReadAllText($logPath)
    Check ($log.Contains('OWNERSHIP_PROBE_PASSED')) 'all 21 production ownership parser cases pass'
    Check ($harness.ExitCode -ne 0) 'ownership probe declines installation'
} finally { $harness.Dispose() }

# Load just the production Start-Launcher function through its parsed AST. The
# verifier's top-level installation and registry code is never executed.
$parseTokens = $null
$parseErrors = $null
$verificationPath = Join-Path $workspacePath 'tools/verify_windows_package.ps1'
$ast = [Management.Automation.Language.Parser]::ParseFile($verificationPath, [ref]$parseTokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw 'Package verifier has PowerShell parse errors.' }
$startFunction = $ast.Find({ param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Start-Launcher'
}, $false)
if (-not $startFunction) { throw 'Production Start-Launcher function not found.' }
. ([scriptblock]::Create($startFunction.Extent.Text))
if (-not ('PackageWindow' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class PackageWindow {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls,string title);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window,out uint pid);
}
'@
}
$fixturePath = Join-Path $testPath "startup-fixture-$token.exe"
$fixtureSource = @'
using System;
using System.IO;
using System.Reflection;
using System.Threading;
class StartupFixture {
    static void Main() {
        if (!File.Exists(Path.ChangeExtension(Assembly.GetExecutingAssembly().Location, ".exit")))
            Thread.Sleep(60000);
    }
}
'@
$fixtureSourcePath = Join-Path $testPath 'startup-fixture.cs'
[IO.File]::WriteAllText($fixtureSourcePath, $fixtureSource, [Text.UTF8Encoding]::new($false))
$csc = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $csc /nologo /target:winexe "/out:$fixturePath" $fixtureSourcePath
if ($LASTEXITCODE -ne 0) { throw 'Startup failure fixture compilation failed.' }
$sentinel = Start-Process -FilePath $fixturePath -WindowStyle Hidden -PassThru
try {
    $null = $sentinel.Handle
    $failure = $null
    try { $null = Start-Launcher $fixturePath } catch { $failure = $_ }
    Check ($null -ne $failure -and $failure.Exception.Message -eq 'Packaged launcher window timed out.') 'window timeout preserves the startup error'
    Check (-not $sentinel.HasExited) 'timeout cleanup preserves another process with the same executable'
    $remaining = @(Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($fixturePath)) -ErrorAction SilentlyContinue)
    try {
        Check (@($remaining | Where-Object { $_.Id -ne $sentinel.Id }).Count -eq 0) 'window timeout leaves no owned fixture process'
    } finally { foreach ($process in $remaining) { $process.Dispose() } }
    [IO.File]::WriteAllText([IO.Path]::ChangeExtension($fixturePath, '.exit'), '')
    $failure = $null
    try { $null = Start-Launcher $fixturePath } catch { $failure = $_ }
    Check ($null -ne $failure -and $failure.Exception.Message -eq 'Packaged launcher exited before creating its window.') 'early exit preserves the startup error'
    Check (-not $sentinel.HasExited) 'early-exit cleanup preserves the other fixture process'
} finally {
    try {
        if (-not $sentinel.HasExited) {
            if (-not [string]::Equals($sentinel.Path, $fixturePath, [StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture ownership changed.' }
            $sentinel.Kill()
            if (-not $sentinel.WaitForExit(10000)) { throw 'Fixture cleanup timed out.' }
        }
    } finally { $sentinel.Dispose() }
    [IO.File]::WriteAllLines((Join-Path $testPath 'checks.txt'), $checks, [Text.UTF8Encoding]::new($false))
}
Write-Host "$($checks.Count) packaging edge checks passed. Evidence: $testPath"

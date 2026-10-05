# Installation lifecycle checks use a unique AppId and a non-Run HKCU namespace.
# Never changes an existing PicoRun installation or enables actual logon startup.
[CmdletBinding()]
param(
    [string]$PackageDirectory = 'dist',
    [string]$Iscc
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ([IntPtr]::Size -ne 8) { throw 'Use 64-bit PowerShell for the x64 installation checks.' }
$workspacePath = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot))
if (-not [IO.Path]::IsPathRooted($PackageDirectory)) { $PackageDirectory = Join-Path $workspacePath $PackageDirectory }
$packagePath = (Resolve-Path -LiteralPath $PackageDirectory).Path
if (-not $Iscc) { $Iscc = Join-Path $workspacePath 'runtime/build-tools/inno-6.7.3/ISCC.exe' }
$Iscc = (Resolve-Path -LiteralPath $Iscc).Path
$metadataFiles = @(Get-ChildItem -LiteralPath $packagePath -Filter 'PicoRun-*-build.json')
if ($metadataFiles.Count -ne 1) { throw 'Choose a package directory containing exactly one build manifest.' }
$metadata = Get-Content -LiteralPath $metadataFiles[0].FullName -Raw | ConvertFrom-Json
$version = $metadata.version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid package version.' }
$versionParts = $version.Split('.')
if ([int]$versionParts[2] -ge 65535) { throw 'Verification needs room for a higher patch version.' }
$upgradeVersion = $versionParts[0] + '.' + $versionParts[1] + '.' + ([int]$versionParts[2] + 1)
$zipPath = Join-Path $packagePath "PicoRun-$version-windows-x64.zip"
$setupPath = Join-Path $packagePath "PicoRun-$version-windows-x64-setup.exe"
$token = [guid]::NewGuid().ToString('N')
$testPath = Join-Path $workspacePath "runtime/probe-packaging/$token"
$installedPath = Join-Path $testPath '安装 目录'
$probeKey = "Software\PicoRun\Verification\Packaging-$token"
$uninstallKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\PicoRun.PackagingProbe.${token}_is1"
$groupName = "PicoRun Packaging Probe $token"
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) "$groupName/PicoRun.lnk"
$desktopPath = Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) "$groupName.lnk"
New-Item -ItemType Directory -Path $testPath -Force | Out-Null
$script:checks = [Collections.Generic.List[string]]::new()
function Check([bool]$Passed, [string]$Description) {
    if (-not $Passed) { throw "FAIL $Description" }
    $script:checks.Add("PASS $Description")
    Write-Host "PASS $Description"
}
function Startup-Snapshot {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Run')
    try {
        if (-not $key -or $key.GetValueNames() -notcontains 'PicoRun') { return 'absent' }
        return [string]$key.GetValueKind('PicoRun') + ':' + ($key.GetValue('PicoRun',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) | ConvertTo-Json -Compress)
    } finally { if ($key) { $key.Dispose() } }
}
function Settings-Snapshot {
    $settingsRoot = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'PicoRun'
    $snapshot = foreach ($name in @('language.txt','theme.txt','english-input.txt','icons.txt','apps-v1.bin')) {
        $file = Join-Path $settingsRoot $name
        if (Test-Path -LiteralPath $file -PathType Leaf) { $name + ':' + (Get-FileHash -LiteralPath $file).Hash } else { $name + ':absent' }
    }
    return $snapshot -join "`n"
}
function Invoke-Installer([string]$Executable, [string[]]$ExtraArguments, [string]$Label, [bool]$ShouldSucceed = $true) {
    $logPath = Join-Path $testPath "$Label.log"
    $arguments = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',('/LOG="' + $logPath + '"')) + $ExtraArguments
    # On Windows -Wait includes the copied second-phase uninstaller process.
    $process = Start-Process -FilePath $Executable -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
    try {
        [IO.File]::WriteAllText((Join-Path $testPath "$Label.exit.txt"), [string]$process.ExitCode)
        if ($ShouldSucceed) { Check ($process.ExitCode -eq 0) "$Label exits successfully" }
        else { Check ($process.ExitCode -ne 0) "$Label refuses a running application" }
    } finally { $process.Dispose() }
}
function Read-InstalledVersion {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($uninstallKey)
    try { if ($key) { return $key.GetValue('DisplayVersion') } } finally { if ($key) { $key.Dispose() } }
    return $null
}
function Set-ProbeStartup([string]$Command) {
    if ($probeKey -notmatch '^Software\\PicoRun\\Verification\\Packaging-[a-f0-9]{32}$') { throw 'Unexpected verification key.' }
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($probeKey)
    try { $key.SetValue('PicoRun',$Command,[Microsoft.Win32.RegistryValueKind]::String); $key.SetValue('OtherValue','keep') } finally { $key.Dispose() }
}
function Read-ProbeStartup {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($probeKey)
    try { if ($key) { return $key.GetValue('PicoRun') } } finally { if ($key) { $key.Dispose() } }
    return $null
}

if (-not ('PackageWindow' -as [type])) {
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class PackageWindow {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls,string title);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent,IntPtr after,string cls,string title);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window,out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", EntryPoint="SendMessageW")] public static extern IntPtr Send(IntPtr window,uint message,IntPtr wp,IntPtr lp);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageW")] public static extern IntPtr Read(IntPtr window,uint message,IntPtr wp,StringBuilder text);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageW")] public static extern IntPtr Write(IntPtr window,uint message,IntPtr wp,string text);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window,uint message,IntPtr wp,IntPtr lp);
}
'@
}
function Start-Launcher([string]$Executable) {
    $Executable = (Resolve-Path -LiteralPath $Executable).Path
    $dataPath = Join-Path $testPath 'user-data'
    $sourcePath = Join-Path $testPath 'empty-source'
    New-Item -ItemType Directory -Path $dataPath,$sourcePath -Force | Out-Null
    $arguments = '--hidden --hotkey Ctrl+Alt+F10 --data-dir "' + $dataPath + '" --source "' + $sourcePath + '"'
    $process = Start-Process -FilePath $Executable -ArgumentList $arguments -WindowStyle Hidden -PassThru
    try {
        # Hold the exact process object before inspecting windows; cleanup never finds
        # a process by name or targets an existing launcher after a startup failure.
        $null = $process.Handle
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        do {
            $window = [PackageWindow]::FindWindow('PicoRun.Native.v1','PicoRun')
            [uint32]$owner = 0
            [void][PackageWindow]::GetWindowThreadProcessId($window,[ref]$owner)
            if ($owner -eq $process.Id) { return @{ Process=$process; Window=$window; Executable=$Executable } }
            if ($process.HasExited) { throw 'Packaged launcher exited before creating its window.' }
            Start-Sleep -Milliseconds 25
        } while ([DateTime]::UtcNow -lt $deadline)
        throw 'Packaged launcher window timed out.'
    } catch {
        $startupFailure = $_
        try {
            if (-not $process.HasExited) {
                if (-not [string]::Equals($process.Path, $Executable, [StringComparison]::OrdinalIgnoreCase)) {
                    throw 'Startup cleanup refused a process whose executable path changed.'
                }
                $process.Kill()
                if (-not $process.WaitForExit(10000)) { throw 'Startup cleanup could not stop its launcher.' }
            }
        } catch {
            throw "Startup failed: $startupFailure Cleanup failed: $_"
        } finally { $process.Dispose() }
        throw $startupFailure
    }
}
function Close-Launcher($Launcher) {
    if ($Launcher.Process.HasExited) { return }
    [uint32]$owner = 0
    [void][PackageWindow]::GetWindowThreadProcessId($Launcher.Window,[ref]$owner)
    if ($owner -ne $Launcher.Process.Id -or $Launcher.Process.Path -ne $Launcher.Executable) { throw 'Launcher ownership changed.' }
    [void][PackageWindow]::PostMessage($Launcher.Window,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
    Check ($Launcher.Process.WaitForExit(10000) -and $Launcher.Process.ExitCode -eq 0) 'packaged launcher closes normally'
    $Launcher.Process.Dispose()
}

$startupBefore = Startup-Snapshot
$settingsBefore = Settings-Snapshot
$launcher = $null
try {
    Check (@(Get-Process picorun -ErrorAction SilentlyContinue).Count -eq 0) 'no existing launcher is disturbed'
    Check (-not (Test-Path -LiteralPath $shortcutPath) -and -not (Test-Path -LiteralPath $desktopPath)) 'verification shortcuts are new'
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $names = @($archive.Entries | ForEach-Object { $_.FullName } | Sort-Object)
        $expected = @($metadata.payload_files | Sort-Object)
        Check (($names -join "`n") -eq ($expected -join "`n")) 'ZIP contains only the five declared payload files'
    } finally { $archive.Dispose() }
    $payloadPath = Join-Path $testPath 'zip'
    [IO.Compression.ZipFile]::ExtractToDirectory($zipPath,$payloadPath)
    $exePath = Join-Path $payloadPath 'picorun.exe'
    Check ((Get-FileHash -LiteralPath $exePath).Hash -eq $metadata.executable_sha256) 'ZIP executable matches the release build'
    Check ((Get-Item -LiteralPath $setupPath).VersionInfo.ProductVersion.Trim() -eq $version) 'production installer exposes the package version'
    $sumLines = Get-Content -LiteralPath (Join-Path $packagePath "PicoRun-$version-SHA256SUMS.txt")
    foreach ($line in $sumLines) {
        if ($line -notmatch '^([a-f0-9]{64})  (PicoRun-[a-zA-Z0-9.\-]+)$') { throw 'Invalid checksum file.' }
        Check ((Get-FileHash -LiteralPath (Join-Path $packagePath $Matches[2])).Hash.ToLowerInvariant() -eq $Matches[1]) 'published artifact checksum matches'
    }
    foreach ($probeVersion in @($version,$upgradeVersion)) {
        $compilerOutput = Join-Path $testPath "build-$probeVersion"
        New-Item -ItemType Directory -Path $compilerOutput -Force | Out-Null
        & $Iscc /Q "/DAppVersion=$probeVersion" "/DPayloadDir=$payloadPath" "/DOutputDir=$compilerOutput" "/DPackagingProbe=$token" "/DProbeDir=$installedPath" (Join-Path $workspacePath 'packaging/windows/PicoRun.iss')
        if ($LASTEXITCODE -ne 0) { throw 'Verification installer compilation failed.' }
    }
    $probeSetup = Join-Path $testPath "build-$version/PicoRun-$version-windows-x64-setup.exe"
    $probeUpgrade = Join-Path $testPath "build-$upgradeVersion/PicoRun-$upgradeVersion-windows-x64-setup.exe"
    Invoke-Installer $probeSetup @('/LANG=chinesesimplified') 'install-default'
    $installedExe = Join-Path $installedPath 'picorun.exe'
    $uninstaller = Join-Path $installedPath 'unins000.exe'
    Check ((Get-FileHash -LiteralPath $installedExe).Hash -eq $metadata.executable_sha256) 'installed executable matches ZIP'
    Check ((Read-InstalledVersion) -eq $version) 'per-user uninstall entry contains the installed version'
    Check (Test-Path -LiteralPath $shortcutPath) 'default install creates Start Menu shortcut'
    Check (-not (Test-Path -LiteralPath $desktopPath)) 'desktop shortcut defaults off'
    Check ($null -eq (Read-ProbeStartup)) 'installation does not enable startup'
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    Check ($shortcut.TargetPath -eq $installedExe -and $shortcut.WorkingDirectory -eq $installedPath) 'Start Menu shortcut targets the installed launcher'
    $command = '"' + $installedExe + '" --hidden --hotkey Ctrl+Alt+P'
    Set-ProbeStartup $command
    $notePath = Join-Path $installedPath 'local-note.txt'
    [IO.File]::WriteAllText($notePath,'keep this unrelated file')
    Invoke-Installer $probeUpgrade @('/TASKS=desktopicon') 'upgrade'
    Check ((Read-InstalledVersion) -eq $upgradeVersion) 'upgrade uses the same uninstall entry'
    Check (Test-Path -LiteralPath $installedExe) 'upgrade preserves the previous Unicode install directory'
    Check ((Read-ProbeStartup) -eq $command) 'upgrade preserves the existing startup command'
    Check (Test-Path -LiteralPath $desktopPath) 'desktop shortcut can be enabled explicitly'
    $launcher = Start-Launcher $installedExe
    Check (-not [PackageWindow]::IsWindowVisible($launcher.Window)) 'installed executable starts hidden'
    Invoke-Installer $probeUpgrade @() 'running-upgrade' $false
    Invoke-Installer $uninstaller @() 'running-uninstall' $false
    Check (Test-Path -LiteralPath $installedExe) 'blocked operations leave the executable intact'
    Check ((Read-ProbeStartup) -eq $command) 'blocked uninstall preserves startup registration'
    Close-Launcher $launcher
    $launcher = $null
    Set-ProbeStartup ('  "' + $installedExe.ToUpperInvariant() + '" --hidden')
    Invoke-Installer $uninstaller @() 'uninstall-owned-startup'
    Check ($null -eq (Read-ProbeStartup)) 'uninstall removes a matching quoted startup path regardless of case'
    Check (-not (Test-Path -LiteralPath $installedExe)) 'uninstall removes installed program'
    Check ($null -eq (Read-InstalledVersion)) 'uninstall removes its application entry'
    Check (-not (Test-Path -LiteralPath $shortcutPath) -and -not (Test-Path -LiteralPath $desktopPath)) 'uninstall removes its shortcuts'
    Check (([IO.File]::ReadAllText($notePath)) -eq 'keep this unrelated file') 'uninstall preserves unrelated files in the install directory'
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($probeKey)
    try { Check ($key.GetValue('OtherValue') -eq 'keep') 'uninstall preserves other registry values' } finally { $key.Dispose() }

    $commandsToPreserve = @(
        '"C:\AnotherCopy\picorun.exe" --hidden',
        ('"' + $installedExe + '.backup" --hidden'),
        ('"' + $installedExe + ' --hidden'),
        ('"' + $installedExe + '"suffix --hidden')
    )
    $caseIndex = 0
    foreach ($otherCommand in $commandsToPreserve) {
        Invoke-Installer $probeSetup @() "reinstall-preserve-$caseIndex"
        Set-ProbeStartup $otherCommand
        Invoke-Installer $uninstaller @() "uninstall-preserve-$caseIndex"
        Check ((Read-ProbeStartup) -eq $otherCommand) "uninstall preserves unrelated or malformed startup case $caseIndex"
        $caseIndex++
    }
    $plainPath = Join-Path $testPath 'installed-ascii'
    Invoke-Installer $probeSetup @(('/DIR="' + $plainPath + '"')) 'install-ascii'
    Set-ProbeStartup ((Join-Path $plainPath 'picorun.exe') + ' --hidden')
    Invoke-Installer (Join-Path $plainPath 'unins000.exe') @() 'uninstall-unquoted-startup'
    Check ($null -eq (Read-ProbeStartup)) 'uninstall also handles an unquoted absolute executable path'

    $launcher = Start-Launcher $exePath
    Check (-not [PackageWindow]::IsWindowVisible($launcher.Window)) 'ZIP executable starts without installation'
    $edit = [PackageWindow]::FindWindowEx($launcher.Window,[IntPtr]::Zero,'Edit',$null)
    Check ($edit -ne [IntPtr]::Zero) 'ZIP executable creates its native input control'
    [void][PackageWindow]::Write($edit,0xc,[IntPtr]::Zero,'微信 wx')
    [void][PackageWindow]::Send($launcher.Window,0x8003,[IntPtr]::Zero,[IntPtr]::Zero)
    $text = [Text.StringBuilder]::new(64)
    [void][PackageWindow]::Read($edit,0xd,[IntPtr]64,$text)
    Check ($text.ToString() -eq '微信 wx') 'packaged native input accepts mixed Chinese and English'
    Close-Launcher $launcher
    $launcher = $null
    Check ((Startup-Snapshot) -eq $startupBefore) 'actual user Run entry is unchanged'
    Check ((Settings-Snapshot) -eq $settingsBefore) 'actual user settings and catalog are unchanged'
    [IO.File]::WriteAllLines((Join-Path $testPath 'checks.txt'),$script:checks,[Text.UTF8Encoding]::new($false))
    Write-Host "$($script:checks.Count) packaging checks passed. Evidence: $testPath"
} catch {
    [IO.File]::WriteAllLines((Join-Path $testPath 'failed-checks.txt'),$script:checks,[Text.UTF8Encoding]::new($false))
    throw
} finally {
    if ($launcher) { Close-Launcher $launcher }
    # Normal uninstall owns removal; never recursively remove an arbitrary installation.
    foreach ($cleanupPath in @($installedPath, (Join-Path $testPath 'installed-ascii'))) {
        if (Test-Path -LiteralPath (Join-Path $cleanupPath 'unins000.exe')) {
            Invoke-Installer (Join-Path $cleanupPath 'unins000.exe') @() 'cleanup-uninstall'
        }
    }
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($probeKey,$true)
    if ($key) {
        try { $key.DeleteValue('PicoRun',$false); $key.DeleteValue('OtherValue',$false) } finally { $key.Dispose() }
        [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKey($probeKey,$false)
    }
}

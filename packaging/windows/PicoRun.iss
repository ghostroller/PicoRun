; Build through tools/package_windows.ps1. AppId must stay stable across versions.
#if Ver < EncodeVer(6, 7, 3)
  #error Inno Setup 6.7.3 or newer is required.
#endif
#ifndef AppVersion
  #error AppVersion is required.
#endif
#ifndef PayloadDir
  #error PayloadDir is required.
#endif
#ifndef OutputDir
  #error OutputDir is required.
#endif

#ifdef PackagingProbe
  ; Verification builds use a separate uninstall entry, shortcuts and non-Run key.
  #ifndef ProbeDir
    #error ProbeDir is required for an isolated verification build.
  #endif
  #define ProductId "PicoRun.PackagingProbe." + PackagingProbe
  #define ProductGroup "PicoRun Packaging Probe " + PackagingProbe
  #define InstallDir ProbeDir
  #define StartupKey "Software\PicoRun\Verification\Packaging-" + PackagingProbe
#else
  #define ProductId "PicoRun.Native"
  #define ProductGroup "PicoRun"
  #define InstallDir "{localappdata}\Programs\PicoRun"
  #define StartupKey "Software\Microsoft\Windows\CurrentVersion\Run"
#endif

[Setup]
AppId={#ProductId}
AppName=PicoRun
AppVersion={#AppVersion}
AppPublisher=PicoRun
DefaultDirName={#InstallDir}
DefaultGroupName={#ProductGroup}
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0
DisableProgramGroupPage=yes
AllowNoIcons=yes
UsePreviousAppDir=yes
UsePreviousTasks=yes
AppMutex=Local\PicoRun.Native.v1
SetupMutex=Local\PicoRun.Setup.v1
CloseApplications=no
RestartApplications=no
WizardStyle=modern
Compression=lzma2/normal
SolidCompression=yes
OutputDir={#OutputDir}
OutputBaseFilename=PicoRun-{#AppVersion}-windows-x64-setup
UninstallDisplayIcon={app}\picorun.exe
VersionInfoVersion={#AppVersion}
VersionInfoDescription=PicoRun Setup
VersionInfoProductName=PicoRun
VersionInfoProductVersion={#AppVersion}

[Languages]
Name: "chinesesimplified"; MessagesFile: "ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Messages]
chinesesimplified.SetupAppRunningError=PicoRun 正在运行。%n%n请先从托盘菜单退出 PicoRun，再点击“确定”继续安装，或点击“取消”退出。
chinesesimplified.UninstallAppRunningError=PicoRun 正在运行。%n%n请先从托盘菜单退出 PicoRun，再点击“确定”继续卸载，或点击“取消”退出。
english.SetupAppRunningError=PicoRun is running.%n%nExit PicoRun from its tray menu, then click OK to continue installing, or Cancel to exit.
english.UninstallAppRunningError=PicoRun is running.%n%nExit PicoRun from its tray menu, then click OK to continue uninstalling, or Cancel to exit.

[CustomMessages]
chinesesimplified.StartupCleanupFailed=无法删除指向此安装目录的自启动注册。请在 Windows 启动应用设置中检查 PicoRun。
english.StartupCleanupFailed=Could not remove the startup registration for this installation. Please check PicoRun in Windows startup settings.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Explicit payload only: no test programs, local catalog, configuration or runtime.
Source: "{#PayloadDir}\picorun.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\third_party\pinyin-data\LICENSE"; DestDir: "{app}\third_party\pinyin-data"; Flags: ignoreversion
Source: "{#PayloadDir}\third_party\inno-setup\LICENSE"; DestDir: "{app}\third_party\inno-setup"; Flags: ignoreversion

[Icons]
Name: "{group}\PicoRun"; Filename: "{app}\picorun.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\{#ProductGroup}"; Filename: "{app}\picorun.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\picorun.exe"; Description: "{cm:LaunchProgram,PicoRun}"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent

[Code]
function CommandExecutable(Command: String): String;
var
  EndPos: Integer;
  Quote: String;
begin
  Result := '';
  Command := Trim(Command);
  if Command = '' then
    Exit;
  if Command[1] = '"' then begin
    Delete(Command, 1, 1);
    // An untyped literal selects the ANSI Pos overload; use Unicode for path indices.
    Quote := '"';
    EndPos := Pos(Quote, Command);
    if EndPos = 0 then
      Exit;
    // A quoted executable must be followed by whitespace or the end of command.
    if EndPos < Length(Command) then
      if (Command[EndPos + 1] <> ' ') and (Command[EndPos + 1] <> #9) then
        Exit;
    Result := Copy(Command, 1, EndPos - 1);
  end else begin
    EndPos := 1;
    while EndPos <= Length(Command) do begin
      if (Command[EndPos] = ' ') or (Command[EndPos] = #9) then
        Break;
      EndPos := EndPos + 1;
    end;
    Result := Copy(Command, 1, EndPos - 1);
  end;
end;

procedure RemoveOwnedStartupRegistration;
var
  Command, Executable, InstalledExe: String;
begin
  // Never create a startup entry. Preserve registrations belonging to another copy.
  Log('Checking startup registration ownership');
  if not RegQueryStringValue(HKCU, '{#StartupKey}', 'PicoRun', Command) then begin
    Log('No readable startup registration');
    Exit;
  end;
  Executable := CommandExecutable(Command);
  if Length(Executable) < 3 then begin
    Log('Startup command has no complete executable');
    Exit;
  end;
  // Only a complete absolute drive path is understood; malformed/unknown data stays.
  if (Executable[2] <> ':') or (Executable[3] <> '\') then begin
    Log('Startup command has no absolute drive path');
    Exit;
  end;
  InstalledExe := ExpandConstant('{app}\picorun.exe');
  if CompareText(ExpandFileName(Executable), ExpandFileName(InstalledExe)) <> 0 then begin
    Log('Startup command belongs to a different installation');
    Exit;
  end;
  if RegDeleteValue(HKCU, '{#StartupKey}', 'PicoRun') then
    Log('Removed startup registration for this installation')
  else begin
    Log('Failed to remove startup registration for this installation');
    SuppressibleMsgBox(CustomMessage('StartupCleanupFailed'), mbError, MB_OK, IDOK);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RemoveOwnedStartupRegistration;
end;

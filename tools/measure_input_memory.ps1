# Real keyboard comparison, only the probe's foreground PicoRun; no app launch/Enter.
# Requires the 500 synthetic shortcuts created by native_probe. Raw paths stay in /runtime.
param([switch]$English, [ValidateRange(1,10)][int]$Runs = 5)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ImeQueryProbe {
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr p, IntPtr after, string c, string t);
 [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, UIntPtr w, IntPtr l);
 [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageW")] public static extern IntPtr SendText(IntPtr h, uint m, UIntPtr w, string l);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint tid);
 [DllImport("user32.dll")] public static extern int GetKeyboardLayoutList(int n, [Out] IntPtr[] list);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern int IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern void keybd_event(byte k, byte scan, uint flags, UIntPtr extra);
 [DllImport("imm32.dll")] public static extern IntPtr ImmGetDefaultIMEWnd(IntPtr h);
 [DllImport("user32.dll")] public static extern int GetWindowRect(IntPtr h, out Rect r);
 [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr p, uint flags);
 [DllImport("psapi.dll")] public static extern bool GetProcessMemoryInfo(IntPtr p, ref Memory m, uint size);
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left,top,right,bottom; }
 [StructLayout(LayoutKind.Sequential)] public struct Memory {
 public uint size, faults; public UIntPtr peakWs,ws,peakPaged,paged,peakNonpaged,nonpaged,pagefile,peakPagefile,priv;
 }
}
'@
Add-Type -AssemblyName System.Drawing
[ImeQueryProbe]::SetProcessDPIAware() | Out-Null
$workspacePath = Split-Path $PSScriptRoot
$probeRoot = Join-Path $workspacePath $(if ($English) { 'runtime/probe-input-english' } else { 'runtime/probe-input-chinese' })
New-Item -ItemType Directory -Path $probeRoot -Force | Out-Null
$sourcePath = Join-Path $workspacePath 'runtime/probe-controlled/应用 入口'
if (-not (Test-Path -LiteralPath $sourcePath)) { throw 'run native_probe to create synthetic shortcuts first' }
$memoryLines = [System.Collections.Generic.List[string]]::new()
$memoryLines.Add('stage,private_bytes,working_set_bytes,peak_commit_bytes,peak_working_set_bytes,gdi_handles,user_handles,cpu_100ns')
$checks = [System.Collections.Generic.List[string]]::new()
function Sample-Stage($process, [string]$stage) {
 $process.Refresh()
 $memory = [ImeQueryProbe+Memory]::new()
 $memory.size = [uint32][Runtime.InteropServices.Marshal]::SizeOf($memory)
 if (-not [ImeQueryProbe]::GetProcessMemoryInfo($process.Handle,[ref]$memory,$memory.size)) { throw 'memory sample failed' }
 $memoryLines.Add("$stage,$($memory.priv.ToUInt64()),$($memory.ws.ToUInt64()),$($memory.peakPagefile.ToUInt64()),$($memory.peakWs.ToUInt64()),$([ImeQueryProbe]::GetGuiResources($process.Handle,0)),$([ImeQueryProbe]::GetGuiResources($process.Handle,1)),$($process.TotalProcessorTime.Ticks)")
}
function Press-Key([byte]$key, [bool]$checkFocus=$true) {
 if ($checkFocus -and [ImeQueryProbe]::GetForegroundWindow() -ne $hwnd) { throw 'own window lost foreground before keyboard injection' }
 [ImeQueryProbe]::keybd_event($key,0,0,[UIntPtr]::Zero)
 [ImeQueryProbe]::keybd_event($key,0,2,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 50
}
function Invoke-OwnHotkey {
 [ImeQueryProbe]::keybd_event(0x11,0,0,[UIntPtr]::Zero)
 [ImeQueryProbe]::keybd_event(0x12,0,0,[UIntPtr]::Zero)
 Press-Key 0x7a $false
 [ImeQueryProbe]::keybd_event(0x12,0,2,[UIntPtr]::Zero)
 [ImeQueryProbe]::keybd_event(0x11,0,2,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 150
}
$runCount = $Runs
for ($run=0; $run -lt $runCount; $run++) {
 $dataPath = Join-Path $probeRoot 'data'
 New-Item -ItemType Directory -Path $dataPath -Force | Out-Null
 [IO.File]::WriteAllText((Join-Path $dataPath 'english-input.txt'), $(if ($English) { "on`n" } else { "off`n" }))
 $arguments = @('--hidden','--hotkey','Ctrl+Alt+F11','--data-dir',('"'+$dataPath+'"'),'--source',('"'+$sourcePath+'"'))
 $process = Start-Process -FilePath (Join-Path $workspacePath 'target/release/picorun.exe') -ArgumentList $arguments -WorkingDirectory $workspacePath -WindowStyle Hidden -PassThru
 $hwnd=[IntPtr]::Zero; $ime=[IntPtr]::Zero; $oldLayout=[IntPtr]::Zero
 try {
  $deadline=[DateTime]::UtcNow.AddSeconds(15)
  do {
   $hwnd=[ImeQueryProbe]::FindWindow('PicoRun.Native.v1','PicoRun')
   [uint32]$ownedPid=0
   $tid=[ImeQueryProbe]::GetWindowThreadProcessId($hwnd,[ref]$ownedPid)
   if ($ownedPid -eq $process.Id) { break }
   if ([DateTime]::UtcNow -gt $deadline) { throw 'own window timeout' }
   Start-Sleep -Milliseconds 5
  } while ($true)
  $edit=[ImeQueryProbe]::FindWindowEx($hwnd,[IntPtr]::Zero,'EDIT',$null)
  Start-Sleep -Milliseconds 100
  Sample-Stage $process "run${run}_hidden"
  $oldLayout=[ImeQueryProbe]::GetKeyboardLayout($tid)
  $layouts=[IntPtr[]]::new(32)
  $layoutCount=[ImeQueryProbe]::GetKeyboardLayoutList($layouts.Length,$layouts)
  $chinese=$layouts[0..($layoutCount-1)] | Where-Object { ($_.ToInt64() -band 0xffff) -eq 0x804 } | Select-Object -First 1
  if (-not $chinese) { throw 'no installed Chinese layout' }
  # Set the test's original layout before showing, so English-on also verifies a real transition.
  [ImeQueryProbe]::SendMessage($hwnd,0x50,[UIntPtr]::Zero,$chinese) | Out-Null
  [ImeQueryProbe]::SendMessage($hwnd,0x8001,[UIntPtr]::Zero,[IntPtr]::Zero) | Out-Null
  Start-Sleep -Milliseconds 150
  if ([ImeQueryProbe]::GetForegroundWindow() -ne $hwnd) { Invoke-OwnHotkey; Invoke-OwnHotkey }
  if ([ImeQueryProbe]::GetForegroundWindow() -ne $hwnd) { throw 'keyboard test requires own foreground window' }
  Sample-Stage $process "run${run}_shown"
  $selected=[ImeQueryProbe]::GetKeyboardLayout($tid)
  if ($English) {
   if (($selected.ToInt64() -band 0x3ff) -ne 9) { throw 'English-on did not select English' }
  } else {
   if ($selected -ne $chinese) { throw 'English-off changed original Chinese layout' }
   $ime=[ImeQueryProbe]::ImmGetDefaultIMEWnd($edit)
   $oldOpen=[ImeQueryProbe]::SendMessage($ime,0x283,[UIntPtr]5,[IntPtr]::Zero)
   $oldConversion=[ImeQueryProbe]::SendMessage($ime,0x283,[UIntPtr]1,[IntPtr]::Zero)
   [ImeQueryProbe]::SendMessage($ime,0x283,[UIntPtr]6,[IntPtr]1) | Out-Null
   [ImeQueryProbe]::SendMessage($ime,0x283,[UIntPtr]2,[IntPtr]1) | Out-Null
  }
  foreach ($key in [Text.Encoding]::ASCII.GetBytes('WEIXIN')) { Press-Key $key }
  Start-Sleep -Milliseconds 1000
  [ImeQueryProbe]::SendMessage($hwnd,0,[UIntPtr]::Zero,[IntPtr]::Zero) | Out-Null
  Sample-Stage $process "run${run}_input"
  if ($run -eq 0) {
   if ([ImeQueryProbe]::GetForegroundWindow() -ne $hwnd) { throw 'capture requires own foreground' }
   [ImeQueryProbe+Rect]$rect = [ImeQueryProbe+Rect]::new()
   [ImeQueryProbe]::GetWindowRect($hwnd,[ref]$rect) | Out-Null
   $bitmap=[Drawing.Bitmap]::new($rect.right-$rect.left,$rect.bottom-$rect.top)
   $graphics=[Drawing.Graphics]::FromImage($bitmap)
   try { $graphics.CopyFromScreen($rect.left,$rect.top,0,0,$bitmap.Size); $bitmap.Save((Join-Path $probeRoot 'composition.png'),[Drawing.Imaging.ImageFormat]::Png) }
   finally { $graphics.Dispose(); $bitmap.Dispose() }
   $checks.Add('modules=' + (($process.Modules | ForEach-Object ModuleName) -join ','))
   $weaselModule = $process.Modules | Where-Object ModuleName -eq 'weasel.dll' | Select-Object -First 1
   if ($weaselModule) { $checks.Add('IME module: ' + $weaselModule.FileVersionInfo.ProductName + ' version=' + $weaselModule.FileVersionInfo.ProductVersion) }
  }
  # Leave preedit pending in the Chinese arm; hide cancels it. No Space/Enter/Shell launch.
  [ImeQueryProbe]::SendMessage($hwnd,0x6,[UIntPtr]::Zero,[IntPtr]::Zero) | Out-Null
  $deadline=[DateTime]::UtcNow.AddSeconds(3)
  while ([ImeQueryProbe]::IsWindowVisible($hwnd) -ne 0) { if ([DateTime]::UtcNow -gt $deadline) { throw 'hide timeout' }; Start-Sleep -Milliseconds 10 }
  if ($English -and [ImeQueryProbe]::GetKeyboardLayout($tid) -ne $chinese) { throw 'hide did not restore test original layout' }
  Sample-Stage $process "run${run}_input_hidden"
  Start-Sleep -Seconds 1
  Sample-Stage $process "run${run}_idle_start"
  Start-Sleep -Seconds 5
  Sample-Stage $process "run${run}_idle_end"
  $checks.Add("PASS run=$run English=$English layout=$($selected.ToInt64().ToString('x')); real six-letter query only; no Enter/launch command")
 } finally {
  if ($ime -ne [IntPtr]::Zero) {
   [ImeQueryProbe]::SendMessage($ime,0x283,[UIntPtr]2,$oldConversion) | Out-Null
   [ImeQueryProbe]::SendMessage($ime,0x283,[UIntPtr]6,$oldOpen) | Out-Null
  }
  if ($oldLayout -ne [IntPtr]::Zero) { [ImeQueryProbe]::SendMessage($hwnd,0x50,[UIntPtr]::Zero,$oldLayout) | Out-Null }
  [ImeQueryProbe]::SendMessage($hwnd,0x10,[UIntPtr]::Zero,[IntPtr]::Zero) | Out-Null
  if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit(); throw 'own probe process failed to close' }
  $memoryLines | Set-Content -LiteralPath (Join-Path $probeRoot 'memory.csv') -Encoding utf8
  $checks | Set-Content -LiteralPath (Join-Path $probeRoot 'checks.txt') -Encoding utf8
  $process.Dispose()
 }
}
"PASS English=$English $runCount-process input probe"

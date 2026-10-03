# Uses an isolated icon-evaluation build, never launches indexed applications.
param([switch]$Real, [switch]$Direct, [switch]$CompareCache, [switch]$ValidateCache, [ValidateRange(1,10)][int]$Runs = 5, [ValidateRange(1,200)][int]$Queries = 200, [string]$Prototype = 'runtime/icon-eval/prototype', [string]$Output = '')
$ErrorActionPreference = 'Stop'
$workspacePath = Split-Path $PSScriptRoot
$evalRoot = Join-Path $workspacePath 'runtime/icon-eval'
$exePath = Join-Path (Join-Path $workspacePath $Prototype) 'target/release/picorun.exe'
if (-not (Test-Path -LiteralPath $exePath)) { throw 'build isolated icon prototype first; see docs/ICON_EVALUATION.md' }
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class IconEvaluation {
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c,string t);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h,uint m,UIntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool InvalidateRect(IntPtr h,IntPtr r,bool erase);
 [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr h,uint kind);
 [DllImport("psapi.dll")] public static extern bool GetProcessMemoryInfo(IntPtr p,ref Memory m,uint size);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern int IsWindowVisible(IntPtr h);
 [StructLayout(LayoutKind.Sequential)] public struct Memory { public uint size,faults; public UIntPtr peakWs,ws,peakPaged,paged,peakNonpaged,nonpaged,pagefile,peakPagefile,priv; }
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left,top,right,bottom; }
}
'@
[IconEvaluation]::SetProcessDPIAware() | Out-Null
$dataset = if ($Real) { 'real' } else { 'synthetic' }
$outputName = if ($Output) { $Output } elseif ($CompareCache) { "cache-$dataset-verified" } elseif ($Direct) { "$dataset-direct" } else { $dataset }
if ($outputName -notmatch '^[a-zA-Z0-9_-]+$') { throw 'output must be a simple evaluation directory name' }
$outputRoot = Join-Path $evalRoot $outputName
if ($CompareCache -and (Test-Path -LiteralPath (Join-Path $outputRoot 'memory.csv'))) { throw 'choose a new output name to preserve previous comparison' }
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
$memoryLines = [Collections.Generic.List[string]]::new()
$memoryLines.Add('run,mode,stage,private,ws,peak_commit,peak_ws,gdi,user,cpu_100ns')
$timingLines = [Collections.Generic.List[string]]::new()
$timingLines.Add('run,mode,phase,sample,ui_ms,ready_ms')
$checkLines = [Collections.Generic.List[string]]::new()
$catalogHash = ''
function Message([uint32]$msg, [uint64]$value=0) { [IconEvaluation]::SendMessage($hwnd,$msg,[UIntPtr]$value,[IntPtr]::Zero).ToInt64() }
function Paint-OwnWindow { [IconEvaluation]::InvalidateRect($hwnd,[IntPtr]::Zero,$false) | Out-Null; Message 0xf | Out-Null }
function Assert-Visible { if ([IconEvaluation]::IsWindowVisible($hwnd) -eq 0) { throw 'own evaluation window became hidden during measurement' } }
function Wait-Icons {
 if ($mode -eq 'off') { return }
 $wait = [Diagnostics.Stopwatch]::StartNew()
 while ((Message 0x8006 0) -ne 1) {
  if ($wait.Elapsed.TotalSeconds -gt 15) { throw 'icon completion timed out' }
  Start-Sleep -Milliseconds 1
 }
 Paint-OwnWindow
}
function Sample-Process([string]$stage) {
 $process.Refresh()
 $memory = [IconEvaluation+Memory]::new()
 $memory.size = [uint32][Runtime.InteropServices.Marshal]::SizeOf($memory)
 if (-not [IconEvaluation]::GetProcessMemoryInfo($process.Handle,[ref]$memory,$memory.size)) { throw 'memory read failed' }
 $memoryLines.Add("$run,$mode,$stage,$($memory.priv.ToUInt64()),$($memory.ws.ToUInt64()),$($memory.peakPagefile.ToUInt64()),$($memory.peakWs.ToUInt64()),$([IconEvaluation]::GetGuiResources($process.Handle,0)),$([IconEvaluation]::GetGuiResources($process.Handle,1)),$($process.TotalProcessorTime.Ticks)")
}
function Record-Stats([string]$stage) {
 if ($mode -ne 'off') {
  $checkLines.Add("run=$run mode=$mode stage=$stage cache=$(Message 0x8006 1) misses=$(Message 0x8006 2) hits=$(Message 0x8006 3) max_load_us=$(Message 0x8006 4) total_load_us=$(Message 0x8006 5) visible_icons=$(Message 0x8006 6) fallbacks=$(Message 0x8006 7) parses=$(Message 0x8006 8) metadata_hits=$(Message 0x8006 9) metadata_entries=$(Message 0x8006 10) metadata_bytes=$(Message 0x8006 11) generic_copies=$(Message 0x8006 12) shared_resources=$(Message 0x8006 13) extracts=$(Message 0x8006 14) invalidations=$(Message 0x8006 15)")
  if ((Message 0x8006 1) -gt 48) { throw 'icon cache exceeded limit' }
  if ($mode -eq 'after' -and ((Message 0x8006 10) -gt 512 -or (Message 0x8006 11) -gt 131072 -or (Message 0x8006 12) -gt 1)) { throw 'optimized cache budget or shared generic invariant failed' }
 }
}
function Screenshot-OwnWindow([string]$path) {
 Add-Type -AssemblyName System.Drawing
 $rect = [IconEvaluation+Rect]::new()
 [IconEvaluation]::GetWindowRect($hwnd,[ref]$rect) | Out-Null
 $bitmap = [Drawing.Bitmap]::new($rect.right-$rect.left,$rect.bottom-$rect.top)
 $graphics = [Drawing.Graphics]::FromImage($bitmap)
 $dc = $graphics.GetHdc()
 try { if (-not [IconEvaluation]::PrintWindow($hwnd,$dc,0)) { throw 'own window capture failed' } }
 finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
 try { $bitmap.Save($path,[Drawing.Imaging.ImageFormat]::Png) } finally { $bitmap.Dispose() }
}
for ($run=0; $run -lt $Runs; $run++) {
 $modes = if ($CompareCache) { switch ($run % 3) { 0 { @('off','before','after') } 1 { @('before','after','off') } 2 { @('after','off','before') } } } elseif ($run % 2 -eq 0) { @('off','on') } else { @('on','off') }
 foreach ($mode in $modes) {
  $dataPath = Join-Path $outputRoot "data-$mode"
  New-Item -ItemType Directory -Path $dataPath -Force | Out-Null
  [IO.File]::WriteAllText((Join-Path $dataPath 'english-input.txt'),"on`n")
  $arguments = @('--hidden','--hotkey','Ctrl+Alt+F12','--data-dir',('"'+$dataPath+'"'),'--theme','light')
  if (-not $Real) { $arguments += @('--source',('"'+(Join-Path $workspacePath 'runtime/icon-eval/source')+'"')) }
  if ($mode -eq 'after') { $arguments += '--icons-cached' } elseif ($mode -eq 'before') { $arguments += '--icons-direct' } elseif ($mode -eq 'on') { $arguments += $(if ($Direct) { '--icons-direct' } else { '--icons' }) }
  $startup = [Diagnostics.Stopwatch]::StartNew()
  $process = Start-Process -FilePath $exePath -ArgumentList $arguments -WorkingDirectory $workspacePath -WindowStyle Hidden -PassThru
  $hwnd = [IntPtr]::Zero
  try {
   while ($true) {
    $hwnd = [IconEvaluation]::FindWindow('PicoRun.IconEval.v1','PicoRun')
    [uint32]$owner = 0
    [IconEvaluation]::GetWindowThreadProcessId($hwnd,[ref]$owner) | Out-Null
    if ($owner -eq $process.Id) { break }
    if ($process.HasExited -or $startup.Elapsed.TotalSeconds -gt 20) { throw 'own icon prototype startup failed' }
    Start-Sleep -Milliseconds 2
   }
   $timingLines.Add("$run,$mode,startup,0,$($startup.Elapsed.TotalMilliseconds),0")
   Start-Sleep -Milliseconds 100
   if ($CompareCache) {
    $currentHash = (Get-FileHash -LiteralPath (Join-Path $dataPath 'apps-v1.bin') -Algorithm SHA256).Hash
    if ($catalogHash -and $catalogHash -ne $currentHash) { throw 'catalog changed between comparison processes' }
    $catalogHash = $currentHash
   }
   Sample-Process 'hidden'
   $show = [Diagnostics.Stopwatch]::StartNew()
   Message 0x8001 | Out-Null
   Paint-OwnWindow
   $textMs = $show.Elapsed.TotalMilliseconds
   Sample-Process 'shown_text'
   Wait-Icons
   Assert-Visible
   $timingLines.Add("$run,$mode,first_show,0,$textMs,$($show.Elapsed.TotalMilliseconds)")
   Start-Sleep -Milliseconds 100
   Sample-Process 'shown_ready'
   Record-Stats 'shown_ready'
   if ($run -eq 0 -and -not $Real) { Screenshot-OwnWindow (Join-Path $outputRoot "$mode.png") }
   # Prewarm the same single result-set; alternating selection would obscure drawing overhead.
   for ($warm=0; $warm -lt 20; $warm++) { Paint-OwnWindow }
   for ($sample=0; $sample -lt 200; $sample++) {
    $watch=[Diagnostics.Stopwatch]::StartNew(); Paint-OwnWindow
    $timingLines.Add("$run,$mode,paint_cached,$sample,$($watch.Elapsed.TotalMilliseconds),0")
   }
   $drawMedian = Message 0x8009 0
   $drawP95 = Message 0x8009 1
   $checkLines.Add("run=$run mode=$mode draw_internal_n=200 draw_p50_us=$drawMedian draw_p95_us=$drawP95 warmup=20")
   Sample-Process 'after_paints'
   $count = [Math]::Min($Queries,(Message 0x8008))
   # Exact entry names are resolved inside the process; personal names/paths are never recorded.
   for ($sample=0; $sample -lt $count; $sample++) {
    $watch=[Diagnostics.Stopwatch]::StartNew()
    if ((Message 0x8007 $sample) -ne 1) { throw 'query entry unavailable' }
    Paint-OwnWindow
    $textMs=$watch.Elapsed.TotalMilliseconds
    Wait-Icons
    $timingLines.Add("$run,$mode,new_results,$sample,$textMs,$($watch.Elapsed.TotalMilliseconds)")
   }
   Sample-Process 'after_queries'
   Assert-Visible
   Record-Stats 'after_queries'
   # Warm interaction with one cache-resident entry, including search, layout, and paint.
   for ($sample=0; $sample -lt 20; $sample++) { Message 0x8007 0 | Out-Null; Paint-OwnWindow; Wait-Icons }
   for ($sample=0; $sample -lt 120; $sample++) {
    $watch=[Diagnostics.Stopwatch]::StartNew(); Message 0x8007 0 | Out-Null; Paint-OwnWindow
    $timingLines.Add("$run,$mode,query_cached,$sample,$($watch.Elapsed.TotalMilliseconds),0")
   }
   for ($sample=0; $sample -lt 1000; $sample++) { Message 0x8007 ($sample % $count) | Out-Null }
   Wait-Icons
   Sample-Process 'after_1000'
   Assert-Visible
   Record-Stats 'after_1000'
   if ($ValidateCache -and $mode -eq 'after') {
    if ((Message 0x8007 0) -ne 1) { throw 'validation entry unavailable' }
    Wait-Icons
    $parseBefore = Message 0x8006 8
    Message 0x8002 | Out-Null # Same refresh handler as F5/tray refresh.
    Wait-Icons
    if ((Message 0x8006 15) -ne 1 -or (Message 0x8006 8) -le $parseBefore) { throw 'cache invalidation did not re-resolve the visible entry' }
    if (-not $Real -and (Message 0x8006 1) -lt 1) { throw 'controlled custom icon lost after refresh' }
    Record-Stats 'validated_invalidation'
    if (-not $Real) { Screenshot-OwnWindow (Join-Path $outputRoot 'validated-after-refresh.png') }
    $checkLines.Add("PASS cache_invalidation_F5 run=$run mode=$mode")
   }
   Message 0x800a | Out-Null
   Start-Sleep -Milliseconds 500
   Sample-Process 'idle_start'
   Start-Sleep -Seconds 2
   Sample-Process 'idle_end'
   $checkLines.Add("PASS run=$run mode=$mode entries=$(Message 0x8008) queried=$count ownWindow=true noAppLaunch=true")
   Message 0x10 | Out-Null
   if (-not $process.WaitForExit(10000)) { throw 'own process shutdown timed out' }
   if ($process.ExitCode -ne 0) { throw 'own process exit failed' }
  } finally {
   if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
   $process.Dispose()
   $memoryLines | Set-Content -LiteralPath (Join-Path $outputRoot 'memory.csv') -Encoding utf8
   $timingLines | Set-Content -LiteralPath (Join-Path $outputRoot 'timings.csv') -Encoding utf8
   $checkLines | Set-Content -LiteralPath (Join-Path $outputRoot 'checks.txt') -Encoding utf8
  }
  "PASS dataset=$dataset run=$run icons=$mode"
 }
}

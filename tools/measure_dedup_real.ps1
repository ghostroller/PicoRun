# Same-machine paired real-catalog comparison; never opens an indexed application.
param([ValidateRange(1,10)][int]$Runs=5)
$ErrorActionPreference='Stop'
$workspacePath=Split-Path $PSScriptRoot
$outputRoot=Join-Path $workspacePath 'runtime/dedup/real'
if(Test-Path (Join-Path $outputRoot 'times.csv')) {throw 'Preserve the existing real comparison before rerunning'}
New-Item -ItemType Directory -Force $outputRoot | Out-Null
$currentExe=Join-Path $workspacePath 'target/release/picorun.exe'
$previousExe=Join-Path $workspacePath 'runtime/dedup/baseline/picorun.exe'
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class DedupRealMeasure {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string c,string t);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr p,IntPtr a,string c,string t);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h,uint m,UIntPtr w,IntPtr l);
 [DllImport("user32.dll",EntryPoint="SendMessageW",CharSet=CharSet.Unicode)] public static extern IntPtr TextMessage(IntPtr h,uint m,UIntPtr w,StringBuilder text);
 [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr h,uint kind);
 [DllImport("user32.dll")] public static extern int IsWindowVisible(IntPtr h);
 [DllImport("psapi.dll")] public static extern bool GetProcessMemoryInfo(IntPtr p,ref Memory m,uint size);
 [StructLayout(LayoutKind.Sequential)] public struct Memory { public uint size,faults; public UIntPtr peakWs,ws,peakPaged,paged,peakNonpaged,nonpaged,pagefile,peakPagefile,priv; }
}
'@
$timings=[Collections.Generic.List[string]]::new()
$timings.Add('version,entries,duplicate_percent,round,catalog_entries,startup_ms,refresh_ms,query_p50_ms,query_p95_ms')
$memoryLines=[Collections.Generic.List[string]]::new()
$memoryLines.Add('version,entries,duplicate_percent,round,stage,private,ws,peak_commit,peak_ws,gdi,user,cpu_100ns')
$checks=[Collections.Generic.List[string]]::new()
$checks.Add("before_sha256=$((Get-FileHash $previousExe).Hash)")
$checks.Add("after_sha256=$((Get-FileHash $currentExe).Hash)")
$originalCount=0
function Message([uint32]$message) { [DedupRealMeasure]::SendMessageW($hwnd,$message,[UIntPtr]::Zero,[IntPtr]::Zero).ToInt64() }
function Sample([string]$stage) {
 $process.Refresh()
 $memory=[DedupRealMeasure+Memory]::new()
 $memory.size=[Runtime.InteropServices.Marshal]::SizeOf($memory)
 if(-not [DedupRealMeasure]::GetProcessMemoryInfo($process.Handle,[ref]$memory,$memory.size)) {throw 'memory sample failed'}
 $memoryLines.Add("$version,$originalCount,real,$round,$stage,$($memory.priv.ToUInt64()),$($memory.ws.ToUInt64()),$($memory.peakPagefile.ToUInt64()),$($memory.peakWs.ToUInt64()),$([DedupRealMeasure]::GetGuiResources($process.Handle,0)),$([DedupRealMeasure]::GetGuiResources($process.Handle,1)),$($process.TotalProcessorTime.Ticks)")
}
for($round=0;$round -lt $Runs;$round++) {
 $order=if($round%2 -eq 0) {@('before','after')} else {@('after','before')}
 foreach($version in $order) {
  $data=Join-Path $outputRoot "data-$version-$round"
  New-Item -ItemType Directory -Force $data | Out-Null
  $exe=if($version -eq 'before') {$previousExe} else {$currentExe}
  $arguments=@('--hidden','--hotkey','Ctrl+Alt+F11','--icons','off','--measure-icons','--hold-measurement-window','--data-dir',('"'+$data+'"'))
  $watch=[Diagnostics.Stopwatch]::StartNew()
  $process=Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $workspacePath -WindowStyle Hidden -PassThru
  $hwnd=[IntPtr]::Zero
  try {
   while($true) {
    $hwnd=[DedupRealMeasure]::FindWindowW('PicoRun.Native.v1','PicoRun')
    [uint32]$owner=0
    [DedupRealMeasure]::GetWindowThreadProcessId($hwnd,[ref]$owner) | Out-Null
    if($owner -eq $process.Id) {break}
    if($process.HasExited -or $watch.Elapsed.TotalSeconds -gt 20) {throw 'own comparison process did not initialize'}
    Start-Sleep -Milliseconds 5
   }
   $count=Message 0x8008
   $startup=$watch.Elapsed.TotalMilliseconds
   if(-not $originalCount) {$originalCount=$count}
   $edit=[DedupRealMeasure]::FindWindowExW($hwnd,[IntPtr]::Zero,'Edit',$null)
   Sample 'hidden'
   Message 0x8001 | Out-Null
   $samples=[Collections.Generic.List[double]]::new()
   for($iteration=0;$iteration -lt 30;$iteration++) {
    foreach($query in @('weixin','wx app','app 000','不存在的应用')) {
     $text=[Text.StringBuilder]::new($query)
     $watch=[Diagnostics.Stopwatch]::StartNew()
     [DedupRealMeasure]::TextMessage($edit,0xc,[UIntPtr]::Zero,$text) | Out-Null
     Message 0x8003 | Out-Null
     Message 0xf | Out-Null
     if($iteration -ge 5) {$samples.Add($watch.Elapsed.TotalMilliseconds)}
    }
   }
   if([DedupRealMeasure]::IsWindowVisible($hwnd) -eq 0) {throw 'comparison window became hidden'}
   $samples.Sort()
   Sample 'visible_queries'
   $watch=[Diagnostics.Stopwatch]::StartNew()
   Message 0x8002 | Out-Null
   $refresh=$watch.Elapsed.TotalMilliseconds
   if((Message 0x8008) -ne $count) {throw 'catalog changed during real comparison'}
   Sample 'refreshed'
   Message 0x800a | Out-Null
   Sample 'idle_start'
   Start-Sleep -Milliseconds 500
   Sample 'idle_end'
   $timings.Add("$version,$originalCount,real,$round,$count,$startup,$refresh,$($samples[49]),$($samples[94])")
   $checks.Add("PASS round=$round version=$version count=$count visible_query_samples=100")
   Message 0x10 | Out-Null
   if(-not $process.WaitForExit(5000)) {throw 'comparison shutdown timed out'}
  } finally {
   if(-not $process.HasExited) {$process.Kill();$process.WaitForExit()}
  }
  [IO.File]::WriteAllLines((Join-Path $outputRoot 'times.csv'),$timings)
  [IO.File]::WriteAllLines((Join-Path $outputRoot 'memory.csv'),$memoryLines)
  [IO.File]::WriteAllLines((Join-Path $outputRoot 'checks.txt'),$checks)
 }
}
Write-Output "Real comparison completed: $Runs processes per version; aggregate results in runtime/dedup/real"

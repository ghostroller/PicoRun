$ErrorActionPreference = 'Stop'
$workspacePath = Split-Path $PSScriptRoot
$evalRoot = Join-Path $workspacePath 'runtime/icon-eval'
function Percentile($values,[double]$p) {
 $ordered = @($values | ForEach-Object {[double]$_} | Sort-Object)
 if (-not $ordered.Count) { return 0 }
 $ordered[[Math]::Ceiling($ordered.Count*$p)-1]
}
$summary = [ordered]@{}
foreach($dataset in @('real','real-direct','synthetic','synthetic-direct')) {
 $rows = @(Import-Csv -LiteralPath (Join-Path $evalRoot "$dataset/memory.csv"))
 $timings = @(Import-Csv -LiteralPath (Join-Path $evalRoot "$dataset/timings.csv"))
 $checks = Get-Content -LiteralPath (Join-Path $evalRoot "$dataset/checks.txt")
 $completedOff = @($checks | Where-Object {$_ -match '^PASS .*mode=off '}).Count
 $completedOn = @($checks | Where-Object {$_ -match '^PASS .*mode=on '}).Count
 if ($completedOff -ne $completedOn -or $completedOn -lt 3) { throw "incomplete dataset: $dataset" }
 $data = [ordered]@{runs=$completedOn; memory=@(); timing=@(); draw=@(); cpu=@()}
 foreach($mode in @('off','on')) {
  foreach($stage in @('hidden','shown_ready','after_queries','after_1000','idle_end')) {
   $items = @($rows | Where-Object {$_.mode -eq $mode -and $_.stage -eq $stage})
   $data.memory += [ordered]@{mode=$mode; stage=$stage; private_p50=(Percentile @($items|ForEach-Object{[double]$_.private/1MB}) .5); private_p95=(Percentile @($items|ForEach-Object{[double]$_.private/1MB}) .95); ws_p50=(Percentile @($items|ForEach-Object{[double]$_.ws/1MB}) .5); ws_p95=(Percentile @($items|ForEach-Object{[double]$_.ws/1MB}) .95); peak_commit_max=(Percentile @($items|ForEach-Object{[double]$_.peak_commit/1MB}) 1); peak_ws_max=(Percentile @($items|ForEach-Object{[double]$_.peak_ws/1MB}) 1)}
  }
  foreach($phase in @('startup','first_show','new_results','query_cached','paint_cached')) {
   $p50=@();$p95=@();$ready50=@();$ready95=@()
   for($run=0;$run -lt $completedOn;$run++) {
    $items = @($timings | Where-Object {[int]$_.run -eq $run -and $_.mode -eq $mode -and $_.phase -eq $phase})
    $p50 += Percentile @($items | ForEach-Object{[double]$_.ui_ms}) .5
    $p95 += Percentile @($items | ForEach-Object{[double]$_.ui_ms}) .95
    $ready50 += Percentile @($items | ForEach-Object{[double]$_.ready_ms}) .5
    $ready95 += Percentile @($items | ForEach-Object{[double]$_.ready_ms}) .95
   }
   $single = $phase -in @('startup','first_show')
   $data.timing += [ordered]@{mode=$mode; phase=$phase; ui_p50=(Percentile $p50 .5); ui_p95=(Percentile $p95 $(if($single){.95}else{.5})); ui_tail_max=(Percentile $p95 1); ready_p50=(Percentile $ready50 .5); ready_p95=(Percentile $ready95 $(if($single){.95}else{.5})); ready_tail_max=(Percentile $ready95 1)}
  }
  $draw50=@();$draw95=@()
  foreach($line in $checks) { if($line -match "mode=$mode draw_internal_n=200 draw_p50_us=(\d+) draw_p95_us=(\d+)"){ $draw50 += [double]$Matches[1]/1000; $draw95 += [double]$Matches[2]/1000 } }
  $data.draw += [ordered]@{mode=$mode; p50=(Percentile $draw50 .5); p95=(Percentile $draw95 .5); tail_max=(Percentile $draw95 1)}
  for($run=0;$run -lt $completedOn;$run++) { $start=$rows|Where-Object {$_.mode -eq $mode -and [int]$_.run -eq $run -and $_.stage -eq 'idle_start'}; $end=$rows|Where-Object {$_.mode -eq $mode -and [int]$_.run -eq $run -and $_.stage -eq 'idle_end'}; $data.cpu += [ordered]@{mode=$mode;run=$run;idle_ms=([double]$end.cpu_100ns-[double]$start.cpu_100ns)/10000} }
 }
 $summary[$dataset]=$data
}
$summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evalRoot 'summary.json') -Encoding utf8
'Icon evaluation summary saved; measurements remain in runtime/icon-eval'

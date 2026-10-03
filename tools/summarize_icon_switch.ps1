param([string]$RealDataset = 'switch-real-final', [string]$SyntheticDataset = 'switch-synthetic-final')
$ErrorActionPreference = 'Stop'
$evalRoot = Join-Path (Split-Path $PSScriptRoot) 'runtime/icon-eval'
function Percentile($values, [double]$p) {
 $sorted = @($values | ForEach-Object {[double]$_} | Sort-Object)
 if (-not $sorted.Count) { throw 'missing measurement samples' }
 $sorted[[Math]::Ceiling($sorted.Count*$p)-1]
}
$summary = [ordered]@{}
foreach ($dataset in @($RealDataset,$SyntheticDataset)) {
 if ($dataset -notmatch '^[a-zA-Z0-9_-]+$') { throw 'invalid dataset name' }
 $rows = @(Import-Csv -LiteralPath (Join-Path $evalRoot "$dataset/memory.csv"))
 $timings = @(Import-Csv -LiteralPath (Join-Path $evalRoot "$dataset/timings.csv"))
 $checks = Get-Content -LiteralPath (Join-Path $evalRoot "$dataset/checks.txt")
 $data = [ordered]@{modes=@()}
 $expectedRuns = 0
 foreach ($mode in @('reference_off','reference_on','off','on')) {
  $runs = @($checks | Where-Object {$_ -match "^PASS run=\d+ mode=$mode "}).Count
  if ($runs -lt 3 -or ($expectedRuns -and $runs -ne $expectedRuns)) { throw "incomplete dataset $dataset mode $mode" }
  $expectedRuns = $runs
  $result = [ordered]@{mode=$mode;runs=$runs;memory=@();timing=@();cpu=@();cache=@()}
  $stages = @('hidden','shown_ready','after_queries','after_1000','idle_end')
  if ($rows | Where-Object {$_.mode -eq $mode -and $_.stage -eq 'disabled_after_on'}) { $stages += @('disabled_after_on','after_disabled_queries') }
  foreach ($stage in $stages) {
   $samples = @($rows | Where-Object {$_.mode -eq $mode -and $_.stage -eq $stage})
   if ($samples.Count -ne $runs) { throw 'missing memory samples' }
   $result.memory += [ordered]@{stage=$stage;private_p50=(Percentile @($samples|ForEach-Object{[double]$_.private/1MB}) .5);private_p95=(Percentile @($samples|ForEach-Object{[double]$_.private/1MB}) .95);ws_p50=(Percentile @($samples|ForEach-Object{[double]$_.ws/1MB}) .5);ws_p95=(Percentile @($samples|ForEach-Object{[double]$_.ws/1MB}) .95);peak_commit_max=(Percentile @($samples|ForEach-Object{[double]$_.peak_commit/1MB}) 1);peak_ws_max=(Percentile @($samples|ForEach-Object{[double]$_.peak_ws/1MB}) 1)}
  }
  $phases = @('startup','first_show','new_results','query_cached','paint_cached')
  if ($timings | Where-Object {$_.mode -eq $mode -and $_.phase -eq 'disabled_again'}) { $phases += 'disabled_again' }
  foreach ($phase in $phases) {
   $ui50=@();$ui95=@();$ready50=@();$ready95=@()
   for ($run=0;$run -lt $runs;$run++) {
    $samples=@($timings|Where-Object {$_.mode -eq $mode -and [int]$_.run -eq $run -and $_.phase -eq $phase})
    $expectedSamples = switch ($phase) { 'new_results' {200} 'disabled_again' {200} 'query_cached' {120} 'paint_cached' {200} default {1} }
    if ($samples.Count -ne $expectedSamples) { throw "unexpected timing sample count $dataset $mode $phase" }
    $ui50 += Percentile @($samples|ForEach-Object{[double]$_.ui_ms}) .5
    $ui95 += Percentile @($samples|ForEach-Object{[double]$_.ui_ms}) .95
    $ready50 += Percentile @($samples|ForEach-Object{[double]$_.ready_ms}) .5
    $ready95 += Percentile @($samples|ForEach-Object{[double]$_.ready_ms}) .95
   }
   $p = if ($phase -in @('startup','first_show')) {.95} else {.5}
   $result.timing += [ordered]@{phase=$phase;ui_p50=(Percentile $ui50 .5);ui_p95=(Percentile $ui95 $p);ui_tail_max=(Percentile $ui95 1);ready_p50=(Percentile $ready50 .5);ready_p95=(Percentile $ready95 $p);ready_tail_max=(Percentile $ready95 1)}
  }
  $draw50=@();$draw95=@()
  foreach ($line in $checks) { if ($line -match "mode=$mode draw_internal_n=200 draw_p50_us=(\d+) draw_p95_us=(\d+)") { $draw50 += [double]$Matches[1]/1000; $draw95 += [double]$Matches[2]/1000 } }
  if ($draw50.Count -ne $runs) { throw 'missing internal draw samples' }
  $result.draw=[ordered]@{p50=(Percentile $draw50 .5);p95=(Percentile $draw95 .5);tail_max=(Percentile $draw95 1)}
  for ($run=0;$run -lt $runs;$run++) {
   $initial = $rows|Where-Object {$_.mode -eq $mode -and [int]$_.run -eq $run -and $_.stage -eq 'hidden'}
   $last = $rows|Where-Object {$_.mode -eq $mode -and [int]$_.run -eq $run -and $_.stage -eq 'after_1000'}
   $idleStart = $rows|Where-Object {$_.mode -eq $mode -and [int]$_.run -eq $run -and $_.stage -eq 'idle_start'}
   $idleEnd = $rows|Where-Object {$_.mode -eq $mode -and [int]$_.run -eq $run -and $_.stage -eq 'idle_end'}
   $result.cpu += [ordered]@{run=$run;active_ms=([double]$last.cpu_100ns-[double]$initial.cpu_100ns)/10000;idle_ms=([double]$idleEnd.cpu_100ns-[double]$idleStart.cpu_100ns)/10000}
   if ($mode -in @('on','reference_on')) {
    foreach ($stage in @('after_queries','after_1000')) {
     $line = @($checks|Where-Object {$_ -match "^run=$run mode=$mode stage=$stage "})
     if ($line.Count -ne 1) { throw 'missing cache stats' }
     $stats=[ordered]@{run=$run;stage=$stage}
     foreach ($match in [regex]::Matches($line[0],'([a-z_]+)=(\d+)')) { $stats[$match.Groups[1].Value]=[double]$match.Groups[2].Value }
     $stats.hit_percent=100*$stats.hits/($stats.hits+$stats.misses)
     $result.cache += $stats
    }
   }
  }
  $result.active_cpu_p50_ms=Percentile @($result.cpu|ForEach-Object {$_.active_ms}) .5
  $result.active_cpu_p95_ms=Percentile @($result.cpu|ForEach-Object {$_.active_ms}) .95
  $data.modes += $result
 }
 $summary[$dataset]=$data
}
$summary | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $evalRoot 'switch-comparison.json') -Encoding utf8
'Icon switch comparison summary saved'

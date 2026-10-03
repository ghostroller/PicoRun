param([string]$Root = (Join-Path $PSScriptRoot '../runtime/flicker'))
$ErrorActionPreference = 'Stop'
function Percentile($Values, [double]$P) {
    $sorted = @($Values | ForEach-Object { [double]$_ } | Sort-Object)
    if ($sorted.Count -eq 0) { throw 'No samples' }
    return $sorted[[math]::Max(0, [math]::Ceiling($P * $sorted.Count) - 1)]
}
$versions = @{}
foreach ($version in @('reference','fixed')) {
    $folder = Join-Path $Root "probe-$version"
    $versions[$version] = @{
        responses = @(Import-Csv (Join-Path $folder 'responses.csv'))
        memory = @(Import-Csv (Join-Path $folder 'memory.csv'))
        checks = @(Get-Content (Join-Path $folder 'checks.txt'))
    }
}
$latency = @()
$memory = @()
foreach ($theme in @('dark','light')) {
    foreach ($icons in @('false','true')) {
        foreach ($phase in @('empty_query','typed_query','first_up','first_down')) {
            $entry = [ordered]@{theme=$theme; icons=$icons; phase=$phase}
            foreach ($version in @('reference','fixed')) {
                $samples = @($versions[$version].responses | Where-Object {
                    $_.theme -eq $theme -and $_.icons -eq $icons -and $_.phase -eq $phase
                })
                $entry[$version] = [ordered]@{
                    n=$samples.Count
                    p50_ms=(Percentile $samples.elapsed_ms .50)
                    p95_ms=(Percentile $samples.elapsed_ms .95)
                    p50_dirty_pixels=(Percentile @($samples | ForEach-Object {
                        ([int]$_.right - [int]$_.left) * ([int]$_.bottom - [int]$_.top)
                    }) .50)
                    edit_dirty_max=($samples.edit_dirty | Measure-Object -Maximum).Maximum
                    edit_paints_max=($samples.edit_paints | Measure-Object -Maximum).Maximum
                    buffer_pixels_max=($samples.buffer_pixels | Measure-Object -Maximum).Maximum
                }
            }
            $latency += $entry
        }
        foreach ($stage in @('visible','after-arrows','idle-after')) {
            $entry = [ordered]@{theme=$theme; icons=$icons; stage=$stage}
            foreach ($version in @('reference','fixed')) {
                $samples = @($versions[$version].memory | Where-Object { $_.stage -like "$theme-$icons-*-$stage" })
                $entry[$version] = [ordered]@{
                    n=$samples.Count
                    private_mib=(Percentile $samples.private_bytes .50)/1MB
                    working_set_mib=(Percentile $samples.working_set_bytes .50)/1MB
                    peak_commit_mib=(Percentile $samples.peak_commit_bytes .50)/1MB
                    peak_working_set_mib=(Percentile $samples.peak_working_set_bytes .50)/1MB
                    peak_commit_max_mib=($samples.peak_commit_bytes | Measure-Object -Maximum).Maximum/1MB
                    peak_working_set_max_mib=($samples.peak_working_set_bytes | Measure-Object -Maximum).Maximum/1MB
                    gdi_median=(Percentile $samples.gdi_handles .50)
                }
            }
            $memory += $entry
        }
    }
}
$summary = [ordered]@{
    percentile='nearest-rank; continuous phases pool 200 keys x 3 independent processes per configuration'
    latency_scope='synchronous Edit key-down/key-up, update-region inspection, and immediate native paint; excludes OS key delivery and display scanout'
    memory_scope='per-stage median of 3 full processes; cumulative peaks at that sample; DPI stress only follows fixed after-arrows samples'
    latency=$latency; memory=$memory
    checks=[ordered]@{
        reference=@($versions.reference.checks | Where-Object { $_ -like 'PASS *' }).Count
        fixed=@($versions.fixed.checks | Where-Object { $_ -like 'PASS *' }).Count
        observations=@($versions.fixed.checks | Where-Object { $_ -like 'OBSERVE *' }).Count
    }
}
$summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $Root 'summary.json')
$latency | Where-Object { $_.phase -in @('empty_query','typed_query') } | ForEach-Object {
    [pscustomobject]@{theme=$_.theme; icons=$_.icons; phase=$_.phase; n=$_.fixed.n;
        old_p50_ms=[math]::Round($_.reference.p50_ms,4); old_p95_ms=[math]::Round($_.reference.p95_ms,4);
        new_p50_ms=[math]::Round($_.fixed.p50_ms,4); new_p95_ms=[math]::Round($_.fixed.p95_ms,4)}
} | ConvertTo-Json
$memory | Where-Object { $_.stage -eq 'after-arrows' } | ForEach-Object {
    [pscustomobject]@{theme=$_.theme; icons=$_.icons;
        old_private_mib=[math]::Round($_.reference.private_mib,3); new_private_mib=[math]::Round($_.fixed.private_mib,3);
        old_ws_mib=[math]::Round($_.reference.working_set_mib,3); new_ws_mib=[math]::Round($_.fixed.working_set_mib,3);
        old_peak_mib=[math]::Round($_.reference.peak_commit_mib,3); new_peak_mib=[math]::Round($_.fixed.peak_commit_mib,3);
        old_gdi=$_.reference.gdi_median; new_gdi=$_.fixed.gdi_median}
} | ConvertTo-Json

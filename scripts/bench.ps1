param(
    [string]$Root = ".",
    [string[]]$Queries = @("error","TODO","config","database","index","search","rust","panic","fn","impl"),
    [int]$Runs = 3,
    [switch]$Warmup,
    [switch]$Release,
    [double]$MaxP95Ms = 0,
    [string]$OutJson = ""
)

$ErrorActionPreference = "Stop"

function Get-PercentileValue {
    param(
        [double[]]$Values,
        [double]$Percentile
    )

    if (-not $Values -or $Values.Count -eq 0) {
        return 0.0
    }

    $sorted = $Values | Sort-Object
    $idx = [Math]::Ceiling($sorted.Count * $Percentile) - 1
    if ($idx -lt 0) { $idx = 0 }
    if ($idx -ge $sorted.Count) { $idx = $sorted.Count - 1 }
    return [double]$sorted[$idx]
}

function Get-Stats {
    param([double[]]$Values)

    if (-not $Values -or $Values.Count -eq 0) {
        return [pscustomobject]@{
            count = 0
            min_ms = 0
            max_ms = 0
            avg_ms = 0
            stddev_ms = 0
            p50_ms = 0
            p95_ms = 0
            p99_ms = 0
        }
    }

    $count = $Values.Count
    $sum = 0.0
    foreach ($v in $Values) { $sum += $v }
    $avg = $sum / $count

    $varianceSum = 0.0
    foreach ($v in $Values) {
        $d = $v - $avg
        $varianceSum += ($d * $d)
    }
    $stddev = [Math]::Sqrt($varianceSum / $count)

    $sorted = $Values | Sort-Object

    return [pscustomobject]@{
        count = $count
        min_ms = [Math]::Round([double]$sorted[0], 2)
        max_ms = [Math]::Round([double]$sorted[$sorted.Count - 1], 2)
        avg_ms = [Math]::Round($avg, 2)
        stddev_ms = [Math]::Round($stddev, 2)
        p50_ms = [Math]::Round((Get-PercentileValue -Values $Values -Percentile 0.50), 2)
        p95_ms = [Math]::Round((Get-PercentileValue -Values $Values -Percentile 0.95), 2)
        p99_ms = [Math]::Round((Get-PercentileValue -Values $Values -Percentile 0.99), 2)
    }
}

if ($Runs -lt 1) {
    throw "Runs must be >= 1"
}

$profile = if ($Release) { "release" } else { "debug" }
$buildArg = if ($Release) { "--release" } else { "" }

Write-Host "[bench] build binary ($profile)"
if ($Release) {
    cargo build -q -p lupa --release
} else {
    cargo build -q -p lupa
}

$bin = "target/$profile/lupa.exe"
if (-not (Test-Path $bin)) {
    throw "Binary not found at $bin"
}

Write-Host "[bench] index build"
$indexStart = Get-Date
& $bin --root $Root index build --json | Out-Null
$indexMs = ((Get-Date) - $indexStart).TotalMilliseconds

if ($Warmup) {
    Write-Host "[bench] warmup pass"
    foreach ($q in $Queries) {
        & $bin --root $Root search $q --json | Out-Null
    }
}

$allLatencies = New-Object System.Collections.Generic.List[double]
$runResults = @()

for ($run = 1; $run -le $Runs; $run++) {
    Write-Host "[bench] run $run/$Runs"
    $runLatencies = @()

    foreach ($q in $Queries) {
        $qStart = Get-Date
        & $bin --root $Root search $q --json | Out-Null
        $ms = ((Get-Date) - $qStart).TotalMilliseconds
        $rounded = [Math]::Round($ms, 2)
        $runLatencies += $rounded
        $allLatencies.Add($rounded)
        Write-Host "[bench] run=$run query='$q' took=${rounded}ms"
    }

    $runStats = Get-Stats -Values $runLatencies
    $runResults += [pscustomobject]@{
        run = $run
        latencies_ms = $runLatencies
        stats = $runStats
    }
}

$overall = Get-Stats -Values $allLatencies.ToArray()

$result = [pscustomobject]@{
    profile = $profile
    root = $Root
    timestamp_utc = [DateTime]::UtcNow.ToString("o")
    index_build_ms = [Math]::Round($indexMs, 2)
    queries = $Queries
    query_count = $Queries.Count
    runs = $Runs
    warmup = [bool]$Warmup
    run_results = $runResults
    overall = $overall
}

$json = $result | ConvertTo-Json -Depth 8
$json

if ($OutJson -ne "") {
    $dir = Split-Path -Parent $OutJson
    if ($dir -and -not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir | Out-Null
    }
    $json | Set-Content -Path $OutJson -Encoding Ascii
    Write-Host "[bench] report saved: $OutJson"
}

if ($MaxP95Ms -gt 0 -and $overall.p95_ms -gt $MaxP95Ms) {
    Write-Error "p95 regression: $($overall.p95_ms)ms > threshold ${MaxP95Ms}ms"
    exit 2
}

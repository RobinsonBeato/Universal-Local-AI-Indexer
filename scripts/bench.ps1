param(
    [string]$Root = ".",
    [string[]]$Queries = @("error","TODO","config","database","index","search","rust","panic","fn","impl")
)

$ErrorActionPreference = "Stop"

Write-Host "[bench] build binary"
cargo build -q -p lupa
$bin = "target/debug/lupa.exe"
if (-not (Test-Path $bin)) {
    throw "No se encontró el binario en $bin"
}

Write-Host "[bench] index build"
$indexStart = Get-Date
& $bin --root $Root index build --json | Out-Null
$indexMs = ((Get-Date) - $indexStart).TotalMilliseconds

$latencies = @()
foreach ($q in $Queries) {
    $qStart = Get-Date
    & $bin --root $Root search $q --json | Out-Null
    $ms = ((Get-Date) - $qStart).TotalMilliseconds
    $latencies += [math]::Round($ms, 2)
    Write-Host "[bench] query='$q' took=${ms}ms"
}

$sorted = $latencies | Sort-Object
$idx = [Math]::Ceiling($sorted.Count * 0.95) - 1
if ($idx -lt 0) { $idx = 0 }
$p95 = $sorted[$idx]

$result = [pscustomobject]@{
    index_build_ms = [math]::Round($indexMs, 2)
    queries = $Queries.Count
    p95_search_ms = $p95
    latencies_ms = $latencies
}

$result | ConvertTo-Json -Depth 5

param(
    [string]$BaseDir = "$env:LOCALAPPDATA\\Lupa",
    [int]$Port = 8088
)

$ErrorActionPreference = "Stop"

$serverPath = Join-Path $BaseDir "runtime\\llama-server.exe"
$modelPath = Join-Path $BaseDir "models\\qwen2.5-0.5b-instruct-q4_k_m.gguf"

if (-not (Test-Path $serverPath)) {
    throw "Missing runtime: $serverPath"
}
if (-not (Test-Path $modelPath)) {
    throw "Missing model: $modelPath"
}

& $serverPath -m $modelPath -c 2048 --host 127.0.0.1 --port $Port

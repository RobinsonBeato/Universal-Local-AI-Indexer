param(
    [string]$ModelUrl = "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf",
    [string]$RuntimeZipUrl = "https://github.com/ggerganov/llama.cpp/releases/latest/download/llama-b4825-bin-win-cpu-x64.zip",
    [string]$BaseDir = "$env:LOCALAPPDATA\\Lupa",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$runtimeDir = Join-Path $BaseDir "runtime"
$modelsDir = Join-Path $BaseDir "models"
$tmpDir = Join-Path $BaseDir "tmp"
$modelPath = Join-Path $modelsDir "qwen2.5-0.5b-instruct-q4_k_m.gguf"
$serverPath = Join-Path $runtimeDir "llama-server.exe"

New-Item -ItemType Directory -Force -Path $runtimeDir, $modelsDir, $tmpDir | Out-Null

if ($Force -or -not (Test-Path $serverPath)) {
    $zipFile = Join-Path $tmpDir "llama-runtime.zip"
    Write-Host "Downloading llama.cpp runtime..."
    Invoke-WebRequest -Uri $RuntimeZipUrl -OutFile $zipFile
    Expand-Archive -Path $zipFile -DestinationPath $runtimeDir -Force
    $found = Get-ChildItem -Path $runtimeDir -Recurse -Filter "llama-server.exe" | Select-Object -First 1
    if (-not $found) {
        throw "llama-server.exe not found after extracting runtime zip."
    }
    if ($found.FullName -ne $serverPath) {
        Copy-Item $found.FullName $serverPath -Force
    }
}

if ($Force -or -not (Test-Path $modelPath)) {
    Write-Host "Downloading Qwen 0.5B GGUF model..."
    Invoke-WebRequest -Uri $ModelUrl -OutFile $modelPath
}

$configPath = Join-Path (Get-Location).Path "config.toml"
if (-not (Test-Path $configPath)) {
    New-Item -Path $configPath -ItemType File -Force | Out-Null
}

$configContent = Get-Content $configPath -Raw
if ($configContent -notmatch "\[qa\]") {
    Add-Content -Path $configPath -Value @"

[qa]
mode = "extractive"
model_path = "%LOCALAPPDATA%\\Lupa\\models\\qwen2.5-0.5b-instruct-q4_k_m.gguf"
endpoint = "http://127.0.0.1:8088"
llama_server_path = "%LOCALAPPDATA%\\Lupa\\runtime\\llama-server.exe"
auto_start_server = true
max_tokens = 256
timeout_ms = 12000
"@
}

Write-Host "Done."
Write-Host "Model:   $modelPath"
Write-Host "Runtime: $serverPath"
Write-Host "config.toml updated with [qa] block if missing."
Write-Host "In GUI chat panel select 'Local AI' mode to test."

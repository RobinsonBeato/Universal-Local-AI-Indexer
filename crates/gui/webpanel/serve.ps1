param(
    [int]$Port = 4173
)

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$listener = New-Object System.Net.HttpListener
$prefix = "http://127.0.0.1:$Port/"
$listener.Prefixes.Add($prefix)
$listener.Start()

Write-Host "Serving webpanel at $prefix"
Write-Host "Press Ctrl+C to stop."

function Get-ContentType([string]$path) {
    switch ([IO.Path]::GetExtension($path).ToLowerInvariant()) {
        ".html" { "text/html; charset=utf-8" }
        ".js" { "application/javascript; charset=utf-8" }
        ".css" { "text/css; charset=utf-8" }
        ".json" { "application/json; charset=utf-8" }
        default { "application/octet-stream" }
    }
}

while ($listener.IsListening) {
    try {
        $ctx = $listener.GetContext()
        $reqPath = $ctx.Request.Url.AbsolutePath.TrimStart("/")
        if ([string]::IsNullOrWhiteSpace($reqPath)) { $reqPath = "index.html" }

        $filePath = Join-Path $root $reqPath
        if (-not (Test-Path $filePath)) {
            $ctx.Response.StatusCode = 404
            $bytes = [Text.Encoding]::UTF8.GetBytes("Not found")
            $ctx.Response.OutputStream.Write($bytes, 0, $bytes.Length)
            $ctx.Response.Close()
            continue
        }

        $bytes = [IO.File]::ReadAllBytes($filePath)
        $ctx.Response.StatusCode = 200
        $ctx.Response.ContentType = Get-ContentType $filePath
        $ctx.Response.ContentLength64 = $bytes.Length
        $ctx.Response.OutputStream.Write($bytes, 0, $bytes.Length)
        $ctx.Response.Close()
    } catch {
        if ($listener.IsListening) {
            Write-Host "serve error: $($_.Exception.Message)"
        }
    }
}

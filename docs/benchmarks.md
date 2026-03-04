# Benchmarks básicos

Este proyecto prioriza dos métricas:

1. Indexar `N` archivos sin fallar.
2. Ejecutar 10 queries y reportar `p95`.

## Script sugerido (PowerShell)

```powershell
./scripts/bench.ps1 -Root . -Queries @(
  "error", "TODO", "config", "database", "index", "search", "rust", "panic", "fn", "impl"
)
```

## Qué mide

- Tiempo total de `index build`.
- Latencia por query de `search --json`.
- `p95` de las 10 búsquedas.

## Notas

- Ejecutar benchmark con índice caliente (segunda corrida) para aproximar objetivo `< 50ms` típico.
- En repos grandes (100k archivos), ajustar `include_extensions` y `max_file_size_bytes` para consistencia.

## Corrida local de referencia (2026-03-04)

Entorno: Windows, benchmark sobre este repositorio.

- `index_build_ms`: `116.56`
- `queries`: `10`
- `p95_search_ms`: `16.15`

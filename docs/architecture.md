# Architecture

## Overview

`lupa` usa una arquitectura de dos capas:

1. `crates/core` (`lupa-core`): indexación, storage y query.
2. `crates/cli` (`lupa`): interfaz de línea de comandos.
3. `crates/gui` (`lupa-gui`): interfaz desktop sobre el mismo core.

## Storage model

### Tantivy index (disk)

- Ubicación: `.lupa/index/`
- Campos:
  - `path` (`STRING | STORED`)
  - `content` (`TEXT | STORED`)

### SQLite metadata

- Ubicación: `.lupa/metadata.db`
- Tabla `files`:
  - `path` (PK)
  - `mtime`
  - `size`
  - `hash` (opcional)
  - `indexed_at`

## Incremental strategy

1. Se recorre el filesystem con `walkdir` aplicando excludes (sin limitar tipos de archivo).
2. Se compara contra metadata previa (`mtime + size`).
3. Si el archivo es chico, se calcula `xxhash` para evitar reindexar cambios espurios.
4. Sólo archivos nuevos/cambiados se vuelven a indexar.
5. Paths eliminados se borran de Tantivy y SQLite.

Nota: se indexan todos los archivos por nombre/ruta. El full-text de contenido se aplica sólo a extensiones de texto configuradas.

## Concurrency

- Preprocesado de documentos (lectura + hash) paralelo con `rayon`.
- Escritura de índice en un único writer de Tantivy (consistente por commit).

## Privacy defaults

Excludes por defecto:

- `node_modules`
- `.git`
- `target`
- `AppData`
- `Program Files`
- `Windows`
- `System32`

## CLI commands

- `lupa index build`: indexación incremental manual.
- `lupa index watch`: loop incremental periódico.
- `lupa search "<query>"`: búsqueda full-text y salida JSON opcional.
- `lupa doctor`: health checks de paths/permisos/index/db.

## JSON output stability

`search --json` devuelve estructura estable:

- `query`
- `total_hits`
- `took_ms`
- `hits[]` con `path`, `score`, `snippet`

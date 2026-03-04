# Universal Local AI Indexer (`lupa`)

Indexador y buscador local de archivos para Windows (también portable a Linux/macOS), offline-first, sin IA y sin servicios externos.

## Objetivos

- Offline-first: sin APIs externas, sin subida de archivos, sin telemetría por defecto.
- Costo $0: sin modelos, embeddings, vector DB ni cloud.
- Ultrarrápido: Tantivy full-text + SQLite metadata + indexación incremental.
- Privacidad por defecto: excludes sensibles preconfigurados.

## Qué indexa

- Todos los tipos de archivo por `nombre` y `ruta` (incluye Word, imágenes, binarios, etc.).
- Contenido full-text para extensiones de texto configuradas (`txt`, `md`, `log`, código, etc.).

## Quickstart

### 1) Build

```bash
cargo build --release -p lupa
```

GUI (desktop):

```bash
cargo run -p lupa-gui
```

### 2) Indexar por primera vez

```bash
cargo run -p lupa -- index build
```

Salida JSON:

```bash
cargo run -p lupa -- index build --json
```

### 3) Buscar

```bash
cargo run -p lupa -- search "error de conexión" --limit 20 --highlight --stats
```

Salida JSON para scripts:

```bash
cargo run -p lupa -- search "query" --json
```

### 4) Modo watch (incremental)

```bash
cargo run -p lupa -- index watch --interval-secs 2
```

### 5) Diagnóstico local

```bash
cargo run -p lupa -- doctor
```

## Comandos CLI

- `lupa index build`
- `lupa index watch`
- `lupa search "<query>" [--json] [--limit N] [--path-prefix ...] [--regex ...] [--highlight] [--stats]`
- `lupa doctor`

## Interfaz gráfica

La app `lupa-gui` ofrece:

- root selector
- `Index Build`
- `Watch Start/Stop`
- `Doctor`
- búsqueda con `limit`, `path_prefix`, `regex`, `highlight`
- panel de resultados + panel de actividad

## Configuración (`config.toml` opcional)

Si existe en la raíz del proyecto, se carga automáticamente.

```toml
excludes = ["node_modules", ".git", "target", "AppData", "Program Files", "Windows", "System32"]
include_extensions = ["txt", "md", "log", "rs", "toml", "json", "js", "ts", "py", "sql"]
max_file_size_bytes = 2097152
hash_small_file_threshold = 65536
threads = 0
```

- `threads = 0`: usa los núcleos disponibles.
- `hash_small_file_threshold`: para archivos chicos calcula `xxhash` y evita reindexar si el contenido no cambió.

## Excludes por defecto (privacidad)

- `node_modules`
- `.git`
- `target`
- `AppData`
- `Program Files`
- `Windows`
- `System32`

## Arquitectura

Resumen: [docs/architecture.md](docs/architecture.md)

## Benchmarks básicos

Guía y script: [docs/benchmarks.md](docs/benchmarks.md)

## Verificación DoD

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

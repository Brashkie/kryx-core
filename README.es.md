<div align="center">

<img src="media/logo.png" alt="Kryx core" width="280" />

**Capa fundacional del ecosistema multimedia [Kryx](https://kryx.dev)**

Buffers zero-copy · Pipelines asíncronas · Sincronización A/V · Rust + napi-rs

[![CI](https://github.com/Brashkie/kryx-core/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/kryx-core/actions)
[![npm version](https://img.shields.io/npm/v/@kryxjs/core?color=cb3837&logo=npm)](https://npmjs.com/package/@kryxjs/core)
[![npm downloads](https://img.shields.io/npm/dm/@kryxjs/core?color=cb3837&logo=npm)](https://npmjs.com/package/@kryxjs/core)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange?logo=rust)](https://www.rust-lang.org)
[![node ≥18](https://img.shields.io/badge/node-%E2%89%A518-3c873a?logo=node.js)](https://nodejs.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-6.0-3178c6?logo=typescript)](https://www.typescriptlang.org/)
[![coverage](https://img.shields.io/badge/coverage-%E2%89%A595%25-brightgreen)](#-testing)

[English](README.md) · **Español** · [Documentación API](https://docs.kryx.dev/core) · [Roadmap](docs/ROADMAP.md) · [Arquitectura](docs/ARCHITECTURE.md) · [Changelog](CHANGELOG.md)

</div>

---

## ¿Qué es esto?

`@kryxjs/core` es la capa fundacional de [Kryx](https://kryx.dev) — un ecosistema multimedia moderno para Node.js. Piénsalo como `libavutil`, pero construido sobre módulos componibles, async nativo y la seguridad de memoria de Rust.

**No incluye** códecs, contenedores ni protocolos de streaming. Esos viven en paquetes dedicados `@kryxjs/*`. Este paquete te da las primitivas que toda herramienta multimedia necesita: buffers, pipelines, timestamps, tipos de error y el puente FFI hacia Zig.

```bash
npm install @kryxjs/core
```

```ts
import { MediaBuffer, Pipeline, PassthroughStage } from '@kryxjs/core'

const pipeline = Pipeline.builder()
  .stage(PassthroughStage)
  .build()

const buf = MediaBuffer.video(rawFrame, /* pts */ 90_000)
const [out] = await pipeline.process(buf)

console.log(out.pts, out.mediaType, out.codecId)
// → 90000 video h264
```

---

## ¿Por qué?

| | |
|---|---|
| 🎯 **Buffers zero-copy** | Los frames son `Bytes` refcontados — clonar es gratis, el slicing no asigna |
| ⚡ **Rendimiento nativo** | Rutas críticas en Rust; códecs y SIMD en Zig (paquetes derivados) |
| 🔌 **Async-first** | Construido sobre `tokio` + ES2022 — backpressure, AbortSignal, async iterables |
| 🧩 **Componible** | Las stages encajan entre sí. Sin monolitos. Cada paquete hace una cosa |
| 🔒 **Type-safe** | TypeScript 6.0 strict + `.d.ts` autogenerados por napi-rs |
| 📦 **Dual-package** | ESM y CJS de primera clase — usa el que tu proyecto use |
| 🌐 **Multiplataforma** | Windows, macOS, Linux — x64 y arm64, glibc y musl |
| 🧪 **Testeado** | 16 tests Rust + 85+ tests TS + smoke dual-package. Cobertura ≥95% |
| 🪶 **Compacto** | < 80 KB descomprimido (sin el `.node`, que es específico por plataforma) |

---

## El ecosistema Kryx

`@kryxjs/core` es la base. Todos los demás paquetes se construyen encima.

```
                          ┌─────────────────┐
                          │      kryx       │  ← SDK unificado (la "fachada FFmpeg")
                          └────────┬────────┘
                                   │
        ┌──────────────────────────┼──────────────────────────┐
        ▼                          ▼                          ▼
┌────────────────┐  ┌──────────────────────┐  ┌─────────────────────┐
│ media-video    │  │ media-audio          │  │ media-subtitles     │
│ media-codecs   │  │ media-ai             │  │ media-stream        │
│ media-gpu      │  │ media-containers     │  │ media-cli           │
└────────┬───────┘  └──────────┬───────────┘  └──────────┬──────────┘
         │                     │                          │
         └─────────────────────┼──────────────────────────┘
                               ▼
                    ┌──────────────────────┐
                    │  @kryxjs/core │   ← estás aquí
                    │   (Rust + napi-rs)    │
                    └──────────┬───────────┘
                               │
                               ▼
                    ┌──────────────────────┐
                    │   Zig low-level       │
                    │   (códecs, SIMD, GPU) │
                    └──────────────────────┘
```

---

## Instalación

```bash
npm install @kryxjs/core
# o
pnpm add @kryxjs/core
# o
yarn add @kryxjs/core
```

Los binarios precompilados se distribuyen para todas las plataformas soportadas vía npm `optionalDependencies`. **No necesitas instalar Rust** — solo si vas a contribuir.

| SO | Arquitectura | sub-paquete npm |
|----|--------------|-----------------|
| 🪟 Windows | x64 | `@kryxjs/core-win32-x64-msvc` |
| 🪟 Windows | arm64 | `@kryxjs/core-win32-arm64-msvc` |
| 🍎 macOS | x64 (Intel) | `@kryxjs/core-darwin-x64` |
| 🍎 macOS | arm64 (Apple Silicon) | `@kryxjs/core-darwin-arm64` |
| 🐧 Linux | x64 (glibc) | `@kryxjs/core-linux-x64-gnu` |
| 🐧 Linux | x64 (musl/Alpine) | `@kryxjs/core-linux-x64-musl` |
| 🐧 Linux | arm64 (glibc) | `@kryxjs/core-linux-arm64-gnu` |

---

## Inicio rápido

### 1. Pipeline pass-through puro

```ts
import { MediaBuffer, Pipeline, PassthroughStage } from '@kryxjs/core'

const pipeline = Pipeline.builder()
  .name('passthrough')
  .stage(PassthroughStage)
  .build()

const buf = MediaBuffer.video(Buffer.from([0x00, 0x00, 0x00, 0x01]), 90_000)
const [out] = await pipeline.process(buf)

console.log(out === buf) // true — zero-copy
```

### 2. Stage personalizado (filtro)

```ts
import { Pipeline, type Stage, type MediaFrameLike } from '@kryxjs/core'

const dropEosStage: Stage = {
  name: 'drop-eos',
  process(frame) {
    return frame.isEos ? [] : [frame]
  },
}

const pipeline = Pipeline.builder()
  .stage(dropEosStage)
  .build()

await pipeline.process({ pts: 0 })          // → [{ pts: 0 }]
await pipeline.process({ pts: 1, isEos: true }) // → []  (descartado)
```

### 3. Multi-stage con tap (logging)

```ts
import {
  MediaBuffer, Pipeline, PassthroughStage,
  CounterStage, tapStage,
} from '@kryxjs/core'

const counter = new CounterStage()

const pipeline = Pipeline.builder()
  .name('demo')
  .stage(PassthroughStage)
  .stage(tapStage('log', (frame, ctx) => {
    console.log(`#${ctx.frameCount} pts=${frame.pts} tipo=${frame.mediaType}`)
  }))
  .stage(counter)
  .build()

await pipeline.start()
for (let i = 0; i < 100; i++) {
  await pipeline.process(MediaBuffer.video(payload, i * 3_000))
}
console.log(`Total: ${counter.count} frames`)
await pipeline.stop()
```

### 4. Fan-out (un frame → muchos)

```ts
const duplicador: Stage = {
  name: 'duplicate',
  process(frame) {
    return [frame, { ...frame, pts: frame.pts! + 1 }]
  },
}

const pipeline = Pipeline.builder().stage(duplicador).build()
const out = await pipeline.process({ pts: 100 })
// out.length === 2, valores pts: [100, 101]
```

### 5. Async iterable (streaming)

```ts
async function* leerFrames(): AsyncIterable<MediaBuffer> {
  while (haySiguiente()) yield await leerSiguienteFrame()
}

for await (const frame of pipeline.processStream(leerFrames())) {
  await sink.escribir(frame)
}
```

### 6. Cancelación con AbortSignal

```ts
const ac = new AbortController()

const pipeline = Pipeline.builder()
  .stage(stageLento)
  .signal(ac.signal)
  .build()

setTimeout(() => ac.abort('cancelado por el usuario'), 5_000)

try {
  await pipeline.process(buf)
} catch (err) {
  if (MediaError.isKind(err, 'timeout')) {
    console.log('Cancelado:', err.cause)
  }
}
```

### 7. Aritmética de Timebase (exacta, sin pérdida de precisión)

```ts
import { Timebase } from '@kryxjs/core'

// Convertir de 90kHz (video MPEG) a milisegundos
const ms = Timebase.VIDEO_90K.rescale(90_000, Timebase.MILLISECOND)
console.log(ms) // 1000

// Timebases personalizados — e.g. 25 fps
const fps25 = Timebase.of(1, 25)
fps25.toSeconds(100) // 4 — 100 frames a 25fps = 4 segundos

// Valores PTS largos usan BigInt internamente — sin drift de float
Timebase.AUDIO_48K.rescale(1_000_000_000, Timebase.MILLISECOND)
// → 20833333 (entero preciso, sin error de redondeo)
```

---

## Conceptos centrales

### `MediaBuffer` — la unidad de datos

Envuelve un buffer de bytes refcontado con metadatos: códec, PTS/DTS, flags de frame, índice de stream, metadatos opcionales por tipo (resolución de video / sample rate de audio).

```ts
const frame = MediaBuffer.video(data, /* pts */ 90_000)

frame.pts          // 90000
frame.codecId      // 'h264'
frame.mediaType    // 'video'
frame.len          // bytes
frame.isEmpty      // false
frame.isKeyframe   // false
frame.isEos        // false
frame.data()       // Buffer — vista zero-copy
```

**Clonar es gratis** — solo incrementa un refcount. Sin `memcpy`.

### `Stage` — unidad de procesamiento

Cada stage recibe un frame y devuelve cero o más frames:

```ts
interface Stage<TFrame = MediaFrameLike> {
  readonly name: string
  process(frame: TFrame, ctx: StageContext): Promise<readonly TFrame[]> | readonly TFrame[]
  onStart?(ctx: { pipelineName: string; signal?: AbortSignal }): Promise<void> | void
  onStop?(): Promise<void> | void
  readonly accepts?: readonly MediaType[]
}
```

| Valor de retorno | Comportamiento |
|------------------|----------------|
| `[frame]` | Pasar a través |
| `[]` | Descartar el frame |
| `[a, b, c]` | Fan-out hacia múltiples frames |
| `Promise<...>` | Stages async soportados nativamente |

### `Pipeline` — composición

```ts
const pipeline = Pipeline.builder()
  .name('transcode')                                  // opcional
  .stage(decoderStage)
  .stage(resizerStage)
  .stage(encoderStage)
  .signal(abortController.signal)                     // opcional
  .onError((err, stageName) => log.error(stageName))  // opcional
  .build()

// Ciclo de vida (opcional pero recomendado para stages con recursos)
await pipeline.start()

// Procesar single, batch o stream
const out      = await pipeline.process(frame)
const batch    = await pipeline.processBatch([f1, f2, f3])
for await (const f of pipeline.processStream(source)) { /* ... */ }

await pipeline.stop()
```

### Jerarquía de errores

Todos los errores derivan de `MediaError` y llevan un discriminante `kind`:

| `kind` | Clase | Usado para |
|--------|-------|------------|
| `'buffer'` | `BufferError` | Estado inválido del buffer, acceso OOB |
| `'pipeline'` | `PipelineError` | Fallos de stage, composición inválida |
| `'io'` | `IoError` | Errores de source/sink |
| `'ffi'` | `FfiError` | Errores del addon nativo / puente Zig |
| `'sync'` | `SyncError` | Drift de sincronización A/V excedió el umbral |
| `'unsupported'` | — | Formato/códec no implementado |
| `'invalid_timestamp'` | — | Validación de PTS falló |
| `'closed'` | — | Operación sobre un recurso cerrado |
| `'timeout'` | — | AbortSignal disparado o deadline excedido |
| `'internal'` | — | Bug — nunca debería pasar |

```ts
try {
  await pipeline.process(buf)
} catch (err) {
  if (MediaError.isKind(err, 'pipeline')) {
    console.error('Stage falló:', err.context, err.cause)
  }
  if (err.isRecoverable) reintentar()
  if (err.isFatal)       crash()
}
```

Los errores son JSON-serializables vía `toJSON()` y llevan cadenas de `cause` ES2022.

---

## Comparación

¿Cómo se compara con herramientas existentes?

| Característica | `@kryxjs/core` | FFmpeg (C) | GStreamer | beamcoder / ffmpeg-static |
|---|---|---|---|---|
| **Lenguaje** | Rust + TypeScript | C | C | Bindings C |
| **Seguridad de memoria** | ✅ Garantizada | ⚠️ Manual | ⚠️ Manual | ⚠️ Heredada de C |
| **Async nativo** | ✅ `async`/`await` | ❌ Callbacks/threads | ⚠️ GLib mainloop | ⚠️ Callbacks |
| **Modular** | ✅ Paquetes npm por feature | ❌ Monolito | ✅ Plugin system | ❌ Monolito |
| **TypeScript** | ✅ Primera clase | ❌ | ❌ | ⚠️ Tipos de comunidad |
| **Buffers zero-copy** | ✅ `Bytes` refcontados | ✅ `AVBufferRef` | ✅ `GstBuffer` | ⚠️ Frecuentemente copia |
| **AbortSignal** | ✅ | ❌ | ❌ | ❌ |
| **Binarios precompilados** | ✅ 7 plataformas | ❌ | ❌ | ✅ |
| **Tamaño de instalación** | ~3 MB (por plataforma) | ~70 MB | ~60 MB | ~70 MB |
| **Alcance** | 📦 Solo primitivas | 🎬 Transcoding completo | 🎬 Pipeline completo | 🎬 Transcoding completo |

**`@kryxjs/core` no reemplaza FFmpeg hoy** — es la base que Kryx construye hacia un conjunto equivalente de capacidades, de forma modular, en Rust+Zig.

---

## Características de rendimiento

Números aproximados de benchmarks locales (Ryzen 7 5800X, Node 20):

| Operación | Tiempo | Notas |
|---|---|---|
| Construcción `MediaBuffer.video()` | ~150 ns | Solo struct + Arc bump |
| `MediaBuffer.clone()` | ~50 ns | Refcount únicamente — sin copia |
| `Pipeline.process()` (10-stage passthrough) | ~3 µs | Mayormente overhead de llamada JS |
| `Timebase.rescale()` | ~200 ns | Camino BigInt en valores grandes |
| Cruce frontera Native ↔ JS | ~400 ns | Llamadas tipadas de napi-rs |

Las cargas reales están dominadas por códec/IO, no por estas primitivas. Mide las tuyas.

### `BufferPool` (0.2.0)

En un pipeline que procesa miles de frames por segundo, alocar y liberar un
buffer por cada frame mantiene al allocator ocupado. `BufferPool` recicla esas
allocaciones — reusar un buffer que ya está "caliente" se saltea por completo el
ciclo malloc + fault de página + free.

```rust
use kc_core::buffer::BufferPool;

let mut pool = BufferPool::new();

let mut buf = pool.acquire(64 * 1024); // limpio, capacidad ≥ 64 KiB
buf.extend_from_slice(frame_bytes);
// ... usar el buffer ...
pool.recycle(buf); // vuelve al pool para el próximo frame

println!("hit rate: {:.1}%", pool.stats().hit_rate() * 100.0);
```

Es **size-bucketed** (buckets potencia de 2, 4 KiB → 4 MiB), con **cap por
bucket** para que un pico de frames grandes no retenga memoria indefinidamente,
y hace fallback a asignación directa por encima del bucket más grande. Medido con
Criterion (`cargo bench`), pool vs. una allocación fresca por frame:

| Carga | Baseline | Pool | Speedup |
|---|---|---|---|
| frame 4 KiB | 48.6 ns | 12.3 ns | ~4× |
| frame 64 KiB | 159.6 ns | 12.0 ns | ~13× |
| frame 1 MiB | 10.3 µs | 12.4 ns | ~830× |
| churn (256 × 64 KiB) | 68.5 µs | 2.62 µs | ~26× |
| carga mixta | 1.19 µs | 82.8 ns | ~14× |

Los números son el mejor caso (100% de reuso); la ganancia real depende del hit
rate, que `PoolStats` reporta.

---

## Desarrollo

### Requisitos

- [Rust ≥1.80](https://rustup.rs) — `rustup install stable`
- Node.js ≥18
- `@napi-rs/cli`: viene como `devDependency`

### Setup local

```bash
git clone https://github.com/Brashkie/kryx-core.git
cd kryx-core
npm install
npm run build:debug   # compila addon Rust (debug) + TypeScript
npm test              # ejecuta todos los tests
```

### Scripts

| Comando | Descripción |
|---|---|
| `npm run build` | Build de producción (Rust release + TypeScript) |
| `npm run build:debug` | Build debug (compila rápido, runtime más lento) |
| `npm run build:native` | Solo el addon Rust |
| `npm run build:ts` | Solo TypeScript |
| `npm test` | Rust + TS + smoke dual-package |
| `npm run test:vitest` | Solo tests TS |
| `npm run test:watch` | Tests TS en modo watch |
| `npm run test:coverage` | Reporte de cobertura (provider v8) |
| `npm run test:coverage:ui` | UI interactiva de cobertura |
| `npm run typecheck` | `tsc --noEmit` |
| `npm run lint` | ESLint |
| `npm run clippy` | Rust clippy |
| `npm run format` | Prettier |
| `npm run format:rust` | rustfmt |
| `npm run examples` | Ejecuta todos los `examples/*` |
| `npm run clean` | Elimina `dist/`, `target/`, `npm/`, `.node` |

### Estructura del proyecto

```
kryx-core/
├── src/                    Fuente TypeScript (API pública)
│   ├── index.ts            Punto de entrada público
│   ├── pipeline.ts         Pipeline + trait Stage
│   ├── types.ts            Timebase, Timestamp, MediaType, CodecId
│   └── error.ts            Jerarquía MediaError
│
├── crates/
│   ├── kc-core/            Core Rust puro (sin deps de Node)
│   │   └── src/
│   │       ├── buffer/     MediaBuffer zero-copy
│   │       ├── pipeline/   Pipeline + trait Stage
│   │       ├── sync/       MasterClock + StreamClock
│   │       ├── io/         MediaSource + MediaSink
│   │       ├── ffi/        Puente Zig (feature-gated)
│   │       ├── types/      Tipos compartidos
│   │       └── utils/
│   │
│   └── kc-node/            Bindings napi-rs → binario .node
│       └── src/lib.rs
│
├── __tests__/              Suite Vitest (100+ tests)
├── examples/               Ejemplos ejecutables CJS/ESM/TS
├── scripts/                Helpers de build (paquetes npm por plataforma)
├── npm/                    Paquetes nativos por plataforma (generados)
├── docs/                   ARCHITECTURE, ROADMAP, CONTRIBUTING, etc.
└── .github/workflows/      CI: matriz de tests + pipeline de release
```

---

## Testing

```bash
npm test
```

Ejecuta tres suites:

1. **16 tests Rust** en `crates/kc-core` — cubriendo buffer, pipeline, types, ffi, sync
2. **85+ tests TypeScript** con Vitest — cubriendo jerarquía de errores, aritmética de Timebase (con caminos de overflow BigInt), ciclo de vida del Pipeline, AbortSignal, fan-out, stages async y bindings nativos mockeados
3. **Smoke tests dual-package** — verificando que los imports CJS y ESM resuelven correctamente

Objetivo de cobertura: **≥95%** en todos los archivos. Abre `coverage/index.html` después de `npm run test:coverage`.

---

## Contribuir

PRs bienvenidos. Lee [CONTRIBUTING.md](CONTRIBUTING.md) para:

- Estilo de código (rustfmt + prettier, enforced)
- Conventional Commits
- Checklist de PR (tests + types + docs + changelog)

Fuera del alcance de este paquete (pertenece a `@kryxjs/*`):
- Implementaciones de códec
- Parseo de contenedores
- Protocolos de streaming
- Modelos AI/ML
- Herramientas CLI

Si tienes dudas, [abre una discussion](https://github.com/Brashkie/kryx-core/discussions) primero.

---

## Roadmap

Ver [docs/ROADMAP.md](docs/ROADMAP.md) para el plan completo.

| Versión | Foco | Target |
|---------|------|--------|
| **0.1.x** | Fundaciones | ✅ Publicado |
| **0.2** | Rendimiento — `BufferPool` + benchmarks | ✅ Publicado |
| **0.2.1** | Integración de `BufferPool` en `MediaBufferMut` | ✅ Publicado |
| **0.3** | Observabilidad — métricas de pipeline + tracing spans | Q3 2026 |
| **0.4** | Abstracciones de buffer GPU + primitivas de aceleración HW | Q4 2026 |
| **0.5** | Tiempo real / streaming (RTP, tipos WebRTC, jitter buffer) | 2027 |
| **1.0** | API estable + todos los paquetes Kryx encima | 2027 |

---

## Preguntas frecuentes

<details>
<summary><b>¿Necesito Rust instalado para usar esto?</b></summary>

No. Los binarios precompilados vienen por npm `optionalDependencies` para todas las plataformas soportadas. Solo `npm install @kryxjs/core` y listo.

Solo necesitas Rust si vas a contribuir o compilar desde el código fuente.
</details>

<details>
<summary><b>¿Por qué dos lenguajes (Rust + Zig)?</b></summary>

Rust se encarga de la orquestación: pipelines, async, networking, plugins, ABI a Node. Zig se encarga de las rutas más calientes: códecs internos, SIMD, shaders GPU. Cada lenguaje gana en su capa.

Ver [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) para el razonamiento completo.
</details>

<details>
<summary><b>¿Cómo se compara con FFmpeg?</b></summary>

FFmpeg es una base de código C brillante, madura y completa del año 2000. `@kryxjs/core` es la **base** de una alternativa moderna construida alrededor de:

- Módulos npm componibles en vez de un monolito
- `async`/`await` nativo (sin callbacks ni threads que manejar)
- Seguridad de memoria por defecto
- Tiempo real, IA y GPU como ciudadanos de primera clase en paquetes posteriores

Este paquete por sí solo no reemplaza FFmpeg — es el kit que los paquetes Kryx usan para construir algo equivalente, gradualmente.
</details>

<details>
<summary><b>¿Está listo para producción?</b></summary>

La API es estable en espíritu pero aún es pre-1.0. Podemos introducir cambios disruptivos entre versiones minor antes de 1.0.

Para uso productivo hoy: **fija la versión exacta** (`"0.1.2"` en lugar de `"^0.1.2"`).

Después de 1.0, aplica semver estricto.
</details>

<details>
<summary><b>¿Cómo es `MediaBuffer` zero-copy?</b></summary>

Internamente es un `bytes::Bytes` (el `Arc<[u8]>` refcontado de Rust). Clonar incrementa el refcount; slicear devuelve una vista nueva sobre la misma asignación. Jamás ocurre un `memcpy` durante el recorrido del pipeline — solo en el sitio original de construcción.

```ts
const a = MediaBuffer.video(payloadHuge, 0)
const b = a   // misma asignación, refcount = 2
const c = b   // misma asignación, refcount = 3
```
</details>

<details>
<summary><b>¿Qué pasa con WASM / soporte de navegador?</b></summary>

Aún no, pero está en el roadmap (post-1.0). El core de Rust es en principio compatible con navegador; la capa napi-rs se reemplazaría con `wasm-bindgen`.
</details>

<details>
<summary><b>¿Cómo reporto un problema de seguridad?</b></summary>

No abras un issue público. Escribe a **security@brashkie.dev** y sigue la [Política de Seguridad](SECURITY.md).
</details>

---

## Licencia

[Apache-2.0](LICENSE).

Copyright © 2026 [Brashkie](https://github.com/Brashkie).

---

<div align="center">

Hecho con 🦀 + ⚡ para la web multimedia moderna.

[Sitio web](https://kryx.dev) · [Issues](https://github.com/Brashkie/kryx-core/issues) · [Discusiones](https://github.com/Brashkie/kryx-core/discussions)

</div>
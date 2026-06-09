/**
 * @kryxjs/core — TypeScript example
 * Run: npm run example:ts
 */

import {
  Pipeline,
  PassthroughStage,
  CounterStage,
  tapStage,
  Timebase,
  Timestamp,
  MediaError,
  nativeAddonVersion,
} from '../src'

async function main(): Promise<void> {
  console.log('=== @kryxjs/core TypeScript example ===\n')

  // Native addon version (requires `npm run build:native:debug`)
  try {
    console.log(`Native addon: ${nativeAddonVersion()}`)
  } catch {
    console.log('Native addon not built yet — run `npm run build:native:debug`')
  }

  // Timebase rescale demo
  const pts90k = 90_000
  const ptsMs = Timebase.VIDEO_90K.rescale(pts90k, Timebase.MILLISECOND)
  console.log(`\nTimebase: ${pts90k} (${Timebase.VIDEO_90K}) → ${ptsMs}ms\n`)

  // Build a pipeline with multiple stages
  const counter = new CounterStage()

  const pipeline = Pipeline.builder()
    .name('demo')
    .stage(PassthroughStage)
    .stage(
      tapStage('logger', (frame, ctx) => {
        console.log(`  [${ctx.pipelineName}] frame #${ctx.frameCount} — pts=${frame.pts ?? 'n/a'}`)
      }),
    )
    .stage(counter)
    .build()

  console.log(`Pipeline "${pipeline.name}": ${pipeline.stageNames.join(' → ')}\n`)

  await pipeline.start()

  for (let i = 0; i < 3; i++) {
    await pipeline.process({ pts: i * 90_000 })
  }

  console.log(`\nProcessed ${counter.count} frames`)

  await pipeline.stop()

  // Error handling demo
  try {
    Timebase.of(-1, 1000)
  } catch (err) {
    if (MediaError.is(err)) {
      console.log(`\nCaught MediaError [${err.kind}]: ${err.message}`)
    }
  }

  console.log('\n✓ Done')
}

main().catch((err) => {
  console.error('Example failed:', err)
  process.exit(1)
})

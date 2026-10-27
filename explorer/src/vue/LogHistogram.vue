<script setup lang="ts">
// The histogram's geometry, stacked by `levels`, drawn on the window its
// buckets answer, and the handlers that zoom the window by brushing it. The
// slot draws it.
import type { Bar, Geometry } from '../core/histogram.js'
import { useExplorerContext, useHistogram, type Histogram } from './parts.js'

const props = defineProps<{ levels?: readonly string[] }>()
defineSlots<{
  default(props: {
    geometry: Geometry
    brushed: { x: number; width: number } | null
    handlers: Histogram['handlers']
    zoomTo: (bar: Bar) => void
  }): unknown
}>()

const context = useExplorerContext('LogHistogram')
const histogram = useHistogram(
  () => context.explorer,
  () => context.answers.buckets.value,
  () => props.levels,
  () => context.answers.bucketsAnswer.value,
)
</script>

<template>
  <slot
    :geometry="histogram.geometry.value"
    :brushed="histogram.brushed.value"
    :handlers="histogram.handlers"
    :zoom-to="histogram.zoomTo"
  />
</template>

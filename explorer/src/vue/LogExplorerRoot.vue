<script setup lang="ts" generic="Row">
// Hands the explorer's state and the answers the host holds to the parts
// drawn inside it. Renders nothing of its own: only its slot.
import { provide } from 'vue'
import { explorerKey } from './parts.js'
import type { LogAnswers } from './useLogFetch.js'
import type { LogExplorer } from './useLogExplorer.js'

const props = defineProps<{ explorer: LogExplorer; answers: LogAnswers<Row> }>()
defineSlots<{ default(props: { explorer: LogExplorer; answers: LogAnswers<Row> }): unknown }>()

provide(explorerKey, {
  get explorer() {
    return props.explorer
  },
  get answers() {
    return props.answers as LogAnswers<unknown>
  },
})
</script>

<template>
  <slot :explorer="explorer" :answers="answers" />
</template>

<script setup lang="ts" generic="Row = unknown">
// The rows read so far, newest first, and whether there are more. The slot
// draws them. The root's answers hold the rows untyped; hand this part the
// same `answers` and its slot types them as the host's rows.
import { computed } from 'vue'
import { useExplorerContext } from './parts.js'
import type { LogAnswers } from './useLogFetch.js'

const props = defineProps<{ answers?: LogAnswers<Row> }>()
defineSlots<{
  default(props: { rows: Row[]; more: boolean; loadMore: () => void; loading: boolean }): unknown
}>()

const context = useExplorerContext('LogRows')
const answers = computed(() => props.answers ?? (context.answers as LogAnswers<Row>))
const more = computed(() => answers.value.next.value != null)
const loadMore = () => context.explorer.more(answers.value.next.value)
</script>

<template>
  <slot :rows="answers.rows.value" :more="more" :load-more="loadMore" :loading="answers.loading.value.rows" />
</template>

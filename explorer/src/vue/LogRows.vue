<script setup lang="ts">
// The rows read so far, newest first, and whether there are more. The slot
// draws them.
import { computed } from 'vue'
import { useExplorerContext } from './parts.js'

defineSlots<{
  default(props: { rows: unknown[]; more: boolean; loadMore: () => void; loading: boolean }): unknown
}>()

const { explorer, answers } = useExplorerContext('LogRows')
const more = computed(() => answers.next.value != null)
const loadMore = () => explorer.more(answers.next.value)
</script>

<template>
  <slot :rows="answers.rows.value" :more="more" :load-more="loadMore" :loading="answers.loading.value.rows" />
</template>

<script setup lang="ts">
// A host of one facet, for dom.test.ts: the root and a part, as a host
// writes them.
import LogExplorerRoot from './LogExplorerRoot.vue'
import LogFacet from './LogFacet.vue'
import type { LogAnswers } from './useLogFetch.js'
import type { LogExplorer } from './useLogExplorer.js'

defineProps<{ explorer: LogExplorer; answers: LogAnswers<unknown>; field: string }>()
</script>

<template>
  <LogExplorerRoot :explorer="explorer" :answers="answers">
    <LogFacet :field="field" v-slot="{ items }">{{ items.map((i) => i.value).join(',') }}</LogFacet>
  </LogExplorerRoot>
</template>

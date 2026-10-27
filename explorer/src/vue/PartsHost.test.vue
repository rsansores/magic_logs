<script setup lang="ts">
// A host of the histogram, the rows and the groups, for dom.test.ts: each
// part's slot written out as text.
import LogExplorerRoot from './LogExplorerRoot.vue'
import LogGroups from './LogGroups.vue'
import LogHistogram from './LogHistogram.vue'
import LogRows from './LogRows.vue'
import type { LogAnswers } from './useLogFetch.js'
import type { LogExplorer } from './useLogExplorer.js'

defineProps<{ explorer: LogExplorer; answers: LogAnswers<{ id: string }> }>()
</script>

<template>
  <LogExplorerRoot :explorer="explorer" :answers="answers">
    <LogHistogram v-slot="{ geometry }">
      <p class="bars">{{ geometry.bars.filter((b) => b.total > 0).length }}</p>
    </LogHistogram>
    <LogRows :answers="answers" v-slot="{ rows, more, loadMore }">
      <p class="rows">{{ rows.map((r) => r.id).join(',') }}</p>
      <button v-if="more" @click="loadMore">more</button>
    </LogRows>
    <LogGroups v-slot="{ groups }">
      <p class="groups">{{ groups.length }}</p>
    </LogGroups>
  </LogExplorerRoot>
</template>

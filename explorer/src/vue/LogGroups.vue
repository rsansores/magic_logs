<script setup lang="ts">
// The largest `top` × field groups, with their first and last row. The slot
// draws them.
import type { GroupRow } from '../core/contract/types.js'
import { useExplorerContext } from './parts.js'

defineSlots<{ default(props: { groups: GroupRow[]; loading: boolean }): unknown }>()

const context = useExplorerContext('LogGroups')
</script>

<template>
  <slot :groups="context.answers.groups.value" :loading="context.answers.loading.value.groups" />
</template>

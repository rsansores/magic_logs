<script setup lang="ts">
// One field's values, counted under every other field's filter, and what
// choosing them does. The slot draws them.
import type { FacetItem } from '../core/facets.js'
import { useExplorerContext, useFacet } from './parts.js'

const props = defineProps<{ field: string }>()
defineSlots<{
  default(props: {
    items: FacetItem[]
    chosen: readonly string[]
    toggle: (value: string) => void
    only: (value: string) => void
    clear: () => void
  }): unknown
}>()

const context = useExplorerContext('LogFacet')
const facet = useFacet(
  () => context.explorer,
  () => context.answers.facets.value,
  () => props.field,
)
</script>

<template>
  <slot
    :items="facet.items.value"
    :chosen="facet.chosen.value"
    :toggle="facet.toggle"
    :only="facet.only"
    :clear="facet.clear"
  />
</template>

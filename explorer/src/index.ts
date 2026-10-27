// magic_logs_explorer: the magic_logs explorer for Vue 3, without its looks.

// The state: the window, the chosen values, the search, live, the pages read
// — kept in memory, in the address bar, or in the host's router.
export { useLogExplorer } from './vue/useLogExplorer.js'
export type { LogExplorer, LogExplorerOptions, FieldOptions } from './vue/useLogExplorer.js'
export { memoryStore, urlStore } from './vue/store.js'
export type { Store } from './vue/store.js'

// The four questions asked with the host's fetch functions, for a host with
// no data layer of its own.
export { useLogFetch } from './vue/useLogFetch.js'
export type { LogAnswers, LogFetchers, Page } from './vue/useLogFetch.js'

// The parts, as composables and as renderless components inside
// `LogExplorerRoot`: the slot draws, with the host's own components.
export { useHistogram, useFacet } from './vue/parts.js'
export type { Histogram, Facet } from './vue/parts.js'
export { default as LogExplorerRoot } from './vue/LogExplorerRoot.vue'
export { default as LogHistogram } from './vue/LogHistogram.vue'
export { default as LogFacet } from './vue/LogFacet.vue'
export { default as LogRows } from './vue/LogRows.vue'
export { default as LogGroups } from './vue/LogGroups.vue'

// Everything framework-free, also as `magic_logs_explorer/core`.
export * from './core/index.js'

// magic_logs_explorer/core: magic_logs' answers, typed, and what an explorer
// works out from them — the window, the histogram's geometry, the facets —
// with no framework.

export type { BucketRow, FacetCountRow, GroupRow, Question, Bucket } from './contract/types.js'
export { text, flag, instant, oneOf, listOf } from './codec.js'
export type { Codec } from './codec.js'
export { defaultPresets, presetOf, rangeOf, steps, stepFor, LONGEST } from './window.js'
export type { Presets, Window, Range, Step } from './window.js'
export { stack, fill, geometry, niceMax, timeTicks, selection } from './histogram.js'
export type { Segment, Bar, Tick, Geometry } from './histogram.js'
export { facetOf, narrowed } from './facets.js'
export type { FacetItem } from './facets.js'

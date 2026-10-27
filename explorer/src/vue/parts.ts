// The parts an explorer is drawn from, without their looks: a histogram
// that zooms by brushing, and a facet to choose values from. The renderless
// components wrap these; a host drawing with its own components calls them.

import {
  computed,
  inject,
  shallowRef,
  toValue,
  type ComputedRef,
  type InjectionKey,
  type MaybeRefOrGetter,
} from 'vue'
import type { Bucket, FacetCountRow } from '../core/contract/types.js'
import { facetOf, type FacetItem } from '../core/facets.js'
import { geometry, selection, type Bar, type Geometry } from '../core/histogram.js'
import type { Range } from '../core/window.js'
import type { LogAnswers } from './useLogFetch.js'
import type { LogExplorer } from './useLogExplorer.js'

export interface Histogram {
  geometry: ComputedRef<Geometry>
  /** The brushed span, snapped to whole steps, as fractions of the plot's
   * width; null when nothing is being brushed. */
  brushed: ComputedRef<{ x: number; width: number } | null>
  /** Bind on the plot's element: pressing, dragging and letting go zooms
   * the window to the span brushed; a press alone, to the bar under it. */
  handlers: {
    onPointerdown(event: PointerEvent): void
    onPointermove(event: PointerEvent): void
    onPointerup(event: PointerEvent): void
    onPointercancel(): void
  }
  /** Zooms the window to one bar: for a bar drawn as a button. */
  zoomTo(bar: Bar): void
}

export function useHistogram(
  explorer: LogExplorer,
  buckets: MaybeRefOrGetter<readonly Bucket[]>,
  levels: MaybeRefOrGetter<readonly string[] | undefined> = undefined,
): Histogram {
  const drawn = computed(() =>
    geometry(toValue(buckets), explorer.range.value, explorer.step.value.ms, toValue(levels) ?? []),
  )
  const brush = shallowRef<{ a: number; b: number } | null>(null)

  const span = (a: number, b: number): Range => selection(explorer.range.value, a, b, explorer.step.value.ms)
  const at = (event: PointerEvent) => {
    const box = (event.currentTarget as Element).getBoundingClientRect()
    return box.width ? (event.clientX - box.left) / box.width : 0
  }

  const brushed = computed(() => {
    if (!brush.value) return null
    const { from, to } = span(brush.value.a, brush.value.b)
    const start = explorer.range.value.from.getTime()
    const width = explorer.range.value.to.getTime() - start
    const x = Math.max((from.getTime() - start) / width, 0)
    return { x, width: Math.min((to.getTime() - start) / width, 1) - x }
  })

  return {
    geometry: drawn,
    brushed,
    handlers: {
      onPointerdown(event) {
        if (event.button !== 0) return
        ;(event.currentTarget as Element).setPointerCapture?.(event.pointerId)
        const f = at(event)
        brush.value = { a: f, b: f }
      },
      onPointermove(event) {
        if (brush.value) brush.value = { ...brush.value, b: at(event) }
      },
      onPointerup() {
        if (!brush.value) return
        const { a, b } = brush.value
        brush.value = null
        explorer.zoom(span(a, b))
      },
      onPointercancel() {
        brush.value = null
      },
    },
    zoomTo(bar) {
      explorer.zoom({ from: new Date(bar.at), to: new Date(bar.end) })
    },
  }
}

export interface Facet {
  items: ComputedRef<FacetItem[]>
  chosen: ComputedRef<readonly string[]>
  toggle(value: string): void
  /** Chooses `value` alone. */
  only(value: string): void
  clear(): void
}

export function useFacet<F extends string>(
  explorer: LogExplorer<F>,
  rows: MaybeRefOrGetter<readonly FacetCountRow[]>,
  field: F,
): Facet {
  const chosen = computed(() => explorer.fields[field].value)
  return {
    items: computed(() => facetOf(toValue(rows), field, chosen.value)),
    chosen,
    toggle: (value) => explorer.toggle(field, value),
    only: (value) => (explorer.fields[field].value = [value]),
    clear: () => (explorer.fields[field].value = []),
  }
}

/** What `LogExplorerRoot` hands its parts. */
export interface ExplorerContext {
  explorer: LogExplorer
  answers: LogAnswers<unknown>
}

export const explorerKey: InjectionKey<ExplorerContext> = Symbol('magic_logs_explorer')

export function useExplorerContext(part: string): ExplorerContext {
  const context = inject(explorerKey, null)
  if (!context) throw new Error(`<${part}> is drawn inside <LogExplorerRoot>`)
  return context
}

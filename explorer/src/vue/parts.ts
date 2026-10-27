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
import type { Answered, LogAnswers } from './useLogFetch.js'
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

/** The histogram of `buckets`, stacked by `levels`, drawn on the window and
 * step they answer (`answered`, as `useLogFetch` keeps them) — the
 * explorer's own when left out — and brushed to zoom the explorer. */
export function useHistogram(
  explorer: MaybeRefOrGetter<LogExplorer>,
  buckets: MaybeRefOrGetter<readonly Bucket[]>,
  levels: MaybeRefOrGetter<readonly string[] | undefined> = undefined,
  answered: MaybeRefOrGetter<Answered | null | undefined> = undefined,
): Histogram {
  const on = computed<Answered>(() => {
    const e = toValue(explorer)
    return toValue(answered) ?? { range: e.range.value, step: e.step.value }
  })
  const drawn = computed(() =>
    geometry(toValue(buckets), on.value.range, on.value.step.ms, toValue(levels) ?? []),
  )
  // A brush is held to the plot it began on: an answer that lands while
  // the pointer is down redraws the bars, not what the brush selects.
  type Held = { a: number; b: number; plot: Range; step: number; until: number }
  const brush = shallowRef<Held | null>(null)

  // Whole steps of the plot, ending no later than the window asked: the
  // plot runs to the end of its last bucket, which may be still to come.
  const span = (b: Held): Range => {
    const { from, to } = selection(b.plot, b.a, b.b, b.step)
    const end = Math.min(to.getTime(), b.until)
    return { from: new Date(Math.min(from.getTime(), end - 1)), to: new Date(end) }
  }
  const at = (event: PointerEvent) => {
    const box = (event.currentTarget as Element).getBoundingClientRect()
    return box.width ? (event.clientX - box.left) / box.width : 0
  }

  const brushed = computed(() => {
    if (!brush.value) return null
    const { from, to } = span(brush.value)
    const start = drawn.value.range.from.getTime()
    const width = drawn.value.range.to.getTime() - start
    // On the plot drawn now, which an answer may have moved under the brush.
    const clamp = (v: number) => Math.min(Math.max(v, 0), 1)
    const x = clamp((from.getTime() - start) / width)
    return { x, width: Math.max(clamp((to.getTime() - start) / width) - x, 0) }
  })

  return {
    geometry: drawn,
    brushed,
    handlers: {
      onPointerdown(event) {
        if (event.button !== 0) return
        ;(event.currentTarget as Element).setPointerCapture?.(event.pointerId)
        const f = at(event)
        brush.value = {
          a: f,
          b: f,
          plot: drawn.value.range,
          step: on.value.step.ms,
          until: on.value.range.to.getTime(),
        }
      },
      onPointermove(event) {
        if (brush.value) brush.value = { ...brush.value, b: at(event) }
      },
      onPointerup() {
        if (!brush.value) return
        const held = brush.value
        brush.value = null
        toValue(explorer).zoom(span(held))
      },
      onPointercancel() {
        brush.value = null
      },
    },
    zoomTo(bar) {
      // The last bar is cut where the window ends: not into the future.
      const end = Math.min(bar.end, on.value.range.to.getTime())
      toValue(explorer).zoom({ from: new Date(bar.at), to: new Date(end) })
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

/** One field's values among `rows` (`facets`' answer), and what choosing
 * them does to the explorer. */
export function useFacet<F extends string>(
  explorer: MaybeRefOrGetter<LogExplorer<F>>,
  rows: MaybeRefOrGetter<readonly FacetCountRow[]>,
  field: MaybeRefOrGetter<F>,
): Facet {
  const chosenRef = () => {
    const e = toValue(explorer)
    const name = toValue(field)
    const ref = e.fields[name]
    if (!ref) throw new Error(`the explorer has no field "${name}": it has ${e.fieldNames.join(', ')}`)
    return ref
  }
  const chosen = computed(() => chosenRef().value)
  return {
    items: computed(() => facetOf(toValue(rows), toValue(field), chosen.value)),
    chosen,
    toggle: (value) => toValue(explorer).toggle(toValue(field), value),
    only: (value) => (chosenRef().value = [value]),
    clear: () => (chosenRef().value = []),
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

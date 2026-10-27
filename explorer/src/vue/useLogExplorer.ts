// What an explorer looks at, without its looks: the window, each field's
// chosen values, the search, whether it is live, and the pages of rows read
// so far. Every question is built from this one state, so the histogram, the
// facets, the rows and the groups always answer the same question. Fetching
// is the host's — through its own data layer, or `useLogFetch`.

import { computed, getCurrentScope, onScopeDispose, ref, watch, type ComputedRef, type Ref } from 'vue'
import { flag, instant, listOf, text, type Codec } from '../core/codec.js'
import type { Question } from '../core/contract/types.js'
import { narrowed as narrows } from '../core/facets.js'
import { defaultPresets, rangeOf, stepFor, type Presets, type Range, type Step } from '../core/window.js'
import { memoryStore, type Store } from './store.js'

export interface FieldOptions {
  /** Which values the field may hold: a value read from the store that it
   * refuses is dropped. Any non-empty text when left out. */
  accept?: (value: string) => boolean
  /** The key it is stored under; the field's name when left out. */
  key?: string
}

export interface LogExplorerOptions<F extends string> {
  /** The fields a reader may choose values of, as magic_logs names them. */
  fields: readonly F[] | Record<F, FieldOptions>
  /** The named windows, by how far back each looks in ms: 1h, 24h, 7d, 30d. */
  presets?: Presets
  /** The window shown first; `24h`, or the first preset. */
  preset?: string
  /** Where the state lives: in memory when left out; `urlStore()` for the
   * address bar, or the host's router. */
  store?: Store
  /** The keys the window, its two instants, the search and live are stored
   * under: `window`, `from`, `to`, `q`, `live`. */
  keys?: Partial<Record<'window' | 'from' | 'to' | 'search' | 'live', string>>
  /** How often a live explorer's window slides to now, in ms (10 s). */
  liveEvery?: number
}

export interface LogExplorer<F extends string = string> {
  /** A preset's name, or `custom` for `from` and `to`. */
  preset: Ref<string>
  from: Ref<string>
  to: Ref<string>
  /** Each field's chosen values. */
  fields: Record<F, Ref<string[]>>
  search: Ref<string>
  live: Ref<boolean>
  /** The presets' names, in order. */
  presets: readonly string[]
  fieldNames: readonly F[]
  range: ComputedRef<Range>
  step: ComputedRef<Step>
  /** What every question shares. */
  question: ComputedRef<Question>
  /** Whether a value is chosen or a search typed. */
  narrowed: ComputedRef<boolean>
  /** The cursors of the pages read so far: `undefined` for the first, then
   * each page's `next`. A changed question starts again from the newest. */
  cursors: Ref<(string | undefined)[]>
  /** Reads the page after `next`, when there is one. */
  more(next: string | null | undefined): void
  /** The window becomes `range`, and stops being live. */
  zoom(range: Range): void
  /** The window becomes a preset (or `custom`, keeping its two instants). */
  choose(preset: string): void
  /** Chooses or unchooses one value of `field`. */
  toggle(field: F, value: string): void
  /** Unchooses every value and clears the search. */
  clear(): void
  /** A named window slides to now: the same question, asked again. */
  refresh(): void
}

export function useLogExplorer<F extends string>(options: LogExplorerOptions<F>): LogExplorer<F> {
  const store = options.store ?? memoryStore
  const presets = options.presets ?? defaultPresets
  const names = Object.keys(presets)
  const initial = options.preset ?? ('24h' in presets ? '24h' : (names[0] ?? 'custom'))
  const key = (k: 'window' | 'from' | 'to' | 'search' | 'live', fallback: string) =>
    options.keys?.[k] ?? fallback
  const presetCodec: Codec<string> = {
    parse: (raw) => (raw === 'custom' || raw in presets ? raw : undefined),
    serialize: (v) => v,
  }

  const preset = store(key('window', 'window'), initial, presetCodec)
  const from = store(key('from', 'from'), '', instant)
  const to = store(key('to', 'to'), '', instant)
  const search = store(key('search', 'q'), '', text)
  const live = store(key('live', 'live'), false, flag)

  const specs: Record<string, FieldOptions> = Array.isArray(options.fields)
    ? Object.fromEntries((options.fields as readonly F[]).map((f) => [f, {}]))
    : (options.fields as Record<F, FieldOptions>)
  const fieldNames = Object.keys(specs) as F[]
  const fields = Object.fromEntries(
    fieldNames.map((f) => [f, store<string[]>(specs[f]!.key ?? f, [], listOf(specs[f]!.accept))]),
  ) as Record<F, Ref<string[]>>

  // The clock a named window ends at: moved on a timer while live, and by
  // `refresh` otherwise, so the window holds still between two questions.
  const now = ref(Date.now())
  let ticking: ReturnType<typeof setInterval> | undefined
  watch(
    live,
    (on) => {
      clearInterval(ticking)
      ticking = undefined
      if (on) {
        now.value = Date.now()
        ticking = setInterval(() => (now.value = Date.now()), options.liveEvery ?? 10_000)
      }
    },
    { immediate: true },
  )
  if (getCurrentScope()) onScopeDispose(() => clearInterval(ticking))

  const range = computed(() =>
    rangeOf({ preset: preset.value, from: from.value, to: to.value }, now.value, presets, initial),
  )
  const step = computed(() => stepFor(range.value.to.getTime() - range.value.from.getTime()))
  const question = computed<Question>(() => {
    const chosen: Record<string, string[]> = {}
    for (const f of fieldNames) if (fields[f].value.length) chosen[f] = fields[f].value
    const text = search.value.trim()
    return {
      from: range.value.from.toISOString(),
      to: range.value.to.toISOString(),
      fields: chosen,
      ...(text ? { search: text } : {}),
    }
  })
  const narrowed = computed(() => narrows(question.value))

  const cursors = ref<(string | undefined)[]>([undefined])
  watch(question, () => (cursors.value = [undefined]))

  return {
    preset,
    from,
    to,
    fields,
    search,
    live,
    presets: names,
    fieldNames,
    range,
    step,
    question,
    narrowed,
    cursors,
    more(next) {
      if (next && !cursors.value.includes(next)) cursors.value = [...cursors.value, next]
    },
    zoom(next) {
      from.value = next.from.toISOString()
      to.value = next.to.toISOString()
      preset.value = 'custom'
      live.value = false
    },
    choose(next) {
      if (next !== 'custom') {
        from.value = ''
        to.value = ''
      } else if (!from.value || !to.value) {
        from.value = range.value.from.toISOString()
        to.value = range.value.to.toISOString()
      }
      preset.value = next
      now.value = Date.now()
    },
    toggle(field, value) {
      const chosen = fields[field].value
      fields[field].value = chosen.includes(value) ? chosen.filter((v) => v !== value) : [...chosen, value]
    },
    clear() {
      for (const f of fieldNames) fields[f].value = []
      search.value = ''
    },
    refresh() {
      now.value = Date.now()
    },
  }
}

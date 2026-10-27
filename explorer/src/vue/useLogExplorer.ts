// What an explorer looks at, without its looks: the window, each field's
// chosen values, the search, whether it is live, and the pages of rows read
// so far. Every question is built from this one state, so the histogram, the
// facets, the rows and the groups always answer the same question. Fetching
// is the host's — through its own data layer, or `useLogFetch`.

import { computed, getCurrentScope, onScopeDispose, ref, watch, type ComputedRef, type Ref } from 'vue'
import { flag, instant, listOf, text, type Codec } from '../core/codec.js'
import type { Question } from '../core/contract/types.js'
import { narrowed as narrows } from '../core/facets.js'
import {
  LONGEST,
  defaultPresets,
  presetOf,
  rangeOf,
  stepFor,
  type Presets,
  type Range,
  type Step,
} from '../core/window.js'
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
  /** The longest a custom window may span, in ms (about two years): a longer
   * one, typed or pasted into the address, reads as the preset shown first. */
  longest?: number
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
  /** Moves each time the clock slides a live window: the only change to
   * the question that is not the reader's. */
  slides: Ref<number>
  /** How often a live window slides, in ms. */
  liveEvery: number
  /** Moves each time the question is asked again as it is (`refresh`): a
   * host's data layer re-asks on it, as `useLogFetch` does. */
  asked: Ref<number>
  /** Reads the page after `next`, when there is one, and stops being live:
   * the window holds still while a reader reads back through it. */
  more(next: string | null | undefined): void
  /** The window becomes `range`, and stops being live. */
  zoom(range: Range): void
  /** The window becomes a preset (or `custom`, holding the window on
   * screen); a name that is not a preset is ignored. */
  choose(preset: string): void
  /** Chooses or unchooses one value of `field`. */
  toggle(field: F, value: string): void
  /** Unchooses every value and clears the search. */
  clear(): void
  /** A named window slides to now, and every question is asked again. */
  refresh(): void
}

export function useLogExplorer<F extends string>(options: LogExplorerOptions<F>): LogExplorer<F> {
  const store = options.store ?? memoryStore
  const presets = options.presets ?? defaultPresets
  const names = Object.keys(presets)
  const initial = options.preset ?? (presetOf(presets, '24h') ? '24h' : (names[0] ?? 'custom'))
  const key = (k: 'window' | 'from' | 'to' | 'search' | 'live', fallback: string) =>
    options.keys?.[k] ?? fallback
  const presetCodec: Codec<string> = {
    parse: (raw) => (raw === 'custom' || presetOf(presets, raw) ? raw : undefined),
    serialize: (v) => v,
  }

  const specs: Record<string, FieldOptions> = Array.isArray(options.fields)
    ? Object.fromEntries((options.fields as readonly F[]).map((f) => [f, {}]))
    : (options.fields as Record<F, FieldOptions>)
  const fieldNames = Object.keys(specs) as F[]
  // Two pieces under one key would write over each other in the store.
  const keys = [
    key('window', 'window'),
    key('from', 'from'),
    key('to', 'to'),
    key('search', 'q'),
    key('live', 'live'),
    ...fieldNames.map((f) => specs[f]!.key ?? f),
  ]
  const twice = keys.find((k, i) => keys.indexOf(k) !== i)
  if (twice !== undefined)
    throw new Error(`the explorer keeps two things under "${twice}": give one a \`key\` of its own`)

  const preset = store(key('window', 'window'), initial, presetCodec)
  const from = store(key('from', 'from'), '', instant)
  const to = store(key('to', 'to'), '', instant)
  const search = store(key('search', 'q'), '', text)
  const live = store(key('live', 'live'), false, flag)

  const fields = Object.fromEntries(
    fieldNames.map((f) => [f, store<string[]>(specs[f]!.key ?? f, [], listOf(specs[f]!.accept))]),
  ) as Record<F, Ref<string[]>>

  // The clock a named window ends at: moved on a timer while live, and by
  // `refresh` otherwise, so the window holds still between two questions.
  const now = ref(Date.now())
  const slides = ref(0)
  const liveEvery = options.liveEvery ?? 10_000
  let ticking: ReturnType<typeof setInterval> | undefined
  watch(
    live,
    (on) => {
      clearInterval(ticking)
      ticking = undefined
      if (on) {
        now.value = Date.now()
        ticking = setInterval(() => {
          now.value = Date.now()
          slides.value++
        }, liveEvery)
      }
    },
    { immediate: true },
  )
  if (getCurrentScope()) onScopeDispose(() => clearInterval(ticking))

  // Each of these changes only when its value does: a write to the store
  // that leaves the question as it was asks nothing again.
  // A custom window the store holds that does not read — no instants,
  // backwards, too long — is the window shown first, said so: on load, and
  // whenever the store moves (the back button, a router).
  const longest = options.longest ?? LONGEST
  // Checked after a write lands whole: zoom and choose write the window's
  // three pieces one at a time, and a half-written window is not one.
  let writing = false
  const normalize = () => {
    if (writing) return
    // A named window keeps no instants: a later `custom` takes the window
    // on screen, not ones left in the address.
    if (preset.value !== 'custom') {
      if (from.value || to.value) {
        from.value = ''
        to.value = ''
      }
      return
    }
    const span = Date.parse(to.value) - Date.parse(from.value)
    if (span > 0 && span <= longest) return
    preset.value = initial
    from.value = ''
    to.value = ''
  }
  const write = (change: () => void) => {
    writing = true
    try {
      change()
    } finally {
      writing = false
    }
    normalize()
  }
  watch([preset, from, to], normalize, { immediate: true, flush: 'sync' })

  const range = computed<Range>((before) => {
    const next = rangeOf(
      { preset: preset.value, from: from.value, to: to.value },
      now.value,
      presets,
      initial,
      longest,
    )
    return before && sameRange(before, next) ? before : next
  })
  const step = computed<Step>((before) => {
    const next = stepFor(range.value.to.getTime() - range.value.from.getTime())
    return before?.name === next.name ? before : next
  })
  const question = computed<Question>((before) => {
    const chosen: Record<string, string[]> = {}
    for (const f of fieldNames) if (fields[f].value.length) chosen[f] = [...fields[f].value]
    const text = search.value.trim()
    const next: Question = {
      from: range.value.from.toISOString(),
      to: range.value.to.toISOString(),
      fields: chosen,
      ...(text ? { search: text } : {}),
    }
    return before && JSON.stringify(before) === JSON.stringify(next) ? before : next
  })
  const narrowed = computed(() => narrows(question.value))

  const cursors = ref<(string | undefined)[]>([undefined])
  watch(question, () => (cursors.value = [undefined]))
  const asked = ref(0)

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
    asked,
    slides,
    liveEvery,
    more(next) {
      if (!next || cursors.value.includes(next)) return
      // Reading back through history holds the window still.
      live.value = false
      cursors.value = [...cursors.value, next]
    },
    zoom(next) {
      // A brush widened past the longest window keeps its end.
      const start = Math.max(next.from.getTime(), next.to.getTime() - longest)
      write(() => {
        from.value = new Date(start).toISOString()
        to.value = next.to.toISOString()
        preset.value = 'custom'
      })
      live.value = false
    },
    choose(next) {
      if (next !== 'custom' && !presetOf(presets, next)) return
      write(() => {
        if (next !== 'custom') {
          from.value = ''
          to.value = ''
        } else if (!from.value || !to.value) {
          from.value = range.value.from.toISOString()
          to.value = range.value.to.toISOString()
        }
        preset.value = next
      })
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
      cursors.value = [undefined]
      asked.value++
    },
  }
}

function sameRange(a: Range, b: Range) {
  return a.from.getTime() === b.from.getTime() && a.to.getTime() === b.to.getTime()
}

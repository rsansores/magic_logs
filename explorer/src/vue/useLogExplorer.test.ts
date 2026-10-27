import { effectScope, nextTick, ref, type Ref } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Codec } from '../core/codec.js'
import type { Store } from './store.js'
import { useLogExplorer } from './useLogExplorer.js'
import { useLogFetch } from './useLogFetch.js'

const now = Date.UTC(2026, 9, 1, 12)
const HOUR = 3_600_000

/** A store over a plain record, standing for an address bar. */
function recordStore(record: Record<string, string>): Store {
  return <T>(key: string, initial: T, codec: Codec<T>) => {
    const raw = record[key]
    const r = ref(raw == null ? initial : (codec.parse(raw) ?? initial)) as Ref<T>
    return r
  }
}

function explorer(record: Record<string, string> = {}) {
  vi.useFakeTimers({ now })
  const scope = effectScope()
  const e = scope.run(() =>
    useLogExplorer({
      fields: { kind: {}, unit: { accept: (v) => /^u\d+$/.test(v) } },
      store: recordStore(record),
    }),
  )!
  return { e, scope }
}

afterEach(() => vi.useRealTimers())

describe('useLogExplorer', () => {
  it('asks for the last 24 hours, nothing chosen, when the store holds nothing', () => {
    const { e, scope } = explorer()
    expect(e.question.value).toEqual({
      from: new Date(now - 24 * HOUR).toISOString(),
      to: new Date(now).toISOString(),
      fields: {},
    })
    expect(e.step.value.name).toBe('hour')
    expect(e.narrowed.value).toBe(false)
    scope.stop()
  })

  it('reads what the store holds, dropping a value a field refuses', () => {
    const { e, scope } = explorer({ window: '1h', unit: 'u1,x,u2', q: ' gate ' })
    expect(e.question.value.fields).toEqual({ unit: ['u1', 'u2'] })
    expect(e.question.value.search).toBe('gate')
    expect(e.step.value.name).toBe('minute')
    scope.stop()
  })

  it('zooms to a range and stops being live; a preset slides again', () => {
    const { e, scope } = explorer({ live: '1' })
    e.zoom({ from: new Date(now - 3 * HOUR), to: new Date(now - 2 * HOUR) })
    expect(e.preset.value).toBe('custom')
    expect(e.live.value).toBe(false)
    expect(e.question.value.from).toBe(new Date(now - 3 * HOUR).toISOString())
    e.choose('7d')
    expect(e.from.value).toBe('')
    expect(e.step.value.name).toBe('sixHours')
    scope.stop()
  })

  it('starts the pages again from the newest when the question changes', async () => {
    const { e, scope } = explorer()
    e.more('c1')
    e.more('c1')
    expect(e.cursors.value).toEqual([undefined, 'c1'])
    e.toggle('kind', 'door')
    await nextTick()
    expect(e.cursors.value).toEqual([undefined])
    e.toggle('kind', 'door')
    expect(e.fields.kind.value).toEqual([])
    scope.stop()
  })

  it('slides a live window with the clock', async () => {
    const { e, scope } = explorer({ live: '1' })
    const before = e.question.value.to
    vi.advanceTimersByTime(10_000)
    await nextTick()
    expect(Date.parse(e.question.value.to) - Date.parse(before)).toBe(10_000)
    scope.stop()
  })
})

describe('useLogFetch', () => {
  it('asks each question, appends the next page, and drops an answer to a question no longer asked', async () => {
    const { e, scope } = explorer()
    let release: (() => void) | undefined
    const answers = scope.run(() =>
      useLogFetch<{ id: string }>(e, {
        histogram: async () => [{ bucket: new Date(now - HOUR).toISOString(), level: 'info', count: 2 }],
        facets: async (_q, fields) => fields.map((field) => ({ field, value: 'x', count: 1 })),
        page: async (q, cursor) => {
          if (q.fields.kind) await new Promise<void>((r) => (release = r))
          return cursor ? { rows: [{ id: 'b' }], next: null } : { rows: [{ id: 'a' }], next: 'c1' }
        },
        grouped: async () => [],
      }),
    )!
    await vi.runAllTimersAsync()
    expect(answers.buckets.value).toEqual([{ at: now - HOUR, counts: { info: 2 } }])
    expect(answers.facets.value.map((f) => f.field)).toEqual(['kind', 'unit'])
    expect(answers.rows.value).toEqual([{ id: 'a' }])
    e.more(answers.next.value)
    await vi.runAllTimersAsync()
    expect(answers.rows.value).toEqual([{ id: 'a' }, { id: 'b' }])
    expect(answers.next.value).toBeNull()

    e.toggle('kind', 'door')
    await vi.runAllTimersAsync()
    expect(answers.loading.value.rows).toBe(true)
    e.toggle('kind', 'door')
    await vi.runAllTimersAsync()
    release!()
    await vi.runAllTimersAsync()
    expect(answers.rows.value).toEqual([{ id: 'a' }])
    expect(answers.loading.value.rows).toBe(false)
    scope.stop()
  })
})

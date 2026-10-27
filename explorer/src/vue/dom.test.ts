// @vitest-environment happy-dom
import { mount } from '@vue/test-utils'
import { effectScope, nextTick, shallowRef, watch } from 'vue'
import type { Step } from '../core/window.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { listOf, text } from '../core/codec.js'
import FacetHost from './FacetHost.test.vue'
import PartsHost from './PartsHost.test.vue'
import LogRows from './LogRows.vue'
import { useFacet, useHistogram } from './parts.js'
import { urlStore } from './store.js'
import { useLogExplorer } from './useLogExplorer.js'
import { useLogFetch, type LogFetchers } from './useLogFetch.js'

const HOUR = 3_600_000
const now = Date.UTC(2026, 9, 1, 12)

function at(path: string) {
  window.history.replaceState(null, '', path)
}

beforeEach(() => {
  vi.useFakeTimers({ now, toFake: ['setTimeout', 'setInterval', 'clearInterval', 'clearTimeout', 'Date'] })
  at('/')
})
afterEach(() => vi.useRealTimers())

function scoped<T>(make: () => T) {
  const scope = effectScope()
  return { value: scope.run(make)!, scope }
}

/** Fetchers that count what they are asked, answering after `delay` ms. */
function counting(delay = 0, page?: LogFetchers<{ id: string }>['page']) {
  const asked = { histogram: 0, facets: 0, page: 0, grouped: 0 }
  const later = <T>(v: T) => new Promise<T>((r) => setTimeout(() => r(v), delay))
  const fetchers: LogFetchers<{ id: string }> = {
    histogram: (_q, step) => (
      asked.histogram++,
      later([{ bucket: new Date(now - HOUR).toISOString(), level: step.name, count: 1 }])
    ),
    facets: () => (asked.facets++, later([{ field: 'kind', value: 'door', count: 3 }])),
    page:
      page ??
      ((_q, cursor) => (
        asked.page++,
        later({ rows: [{ id: cursor ?? 'first' }], next: cursor ? null : 'c1' })
      )),
    grouped: () => (asked.grouped++, later([])),
  }
  return { asked, fetchers }
}

describe('urlStore', () => {
  it('reads the address, writes it back in place, and leaves out what equals its start', () => {
    at('/explore?q=gate&kind=a,b#top')
    const { value: store, scope } = scoped(() => urlStore())
    const q = store('q', '', text)
    const kind = store('kind', [], listOf())
    expect(q.value).toBe('gate')
    expect(kind.value).toEqual(['a', 'b'])
    const length = window.history.length
    q.value = 'seal'
    expect(window.location.search).toBe('?q=seal&kind=a%2Cb')
    expect(window.location.hash).toBe('#top')
    kind.value = []
    expect(window.location.search).toBe('?q=seal')
    expect(window.history.length).toBe(length)
    scope.stop()
  })

  it('follows the back button', async () => {
    const { value: store, scope } = scoped(() => urlStore())
    const q = store('q', '', text)
    at('/?q=back')
    window.dispatchEvent(new PopStateEvent('popstate'))
    await nextTick()
    expect(q.value).toBe('back')
    scope.stop()
  })

  it('does not move a key when another is written', async () => {
    const { value, scope } = scoped(() => {
      const store = urlStore()
      const q = store('q', '', text)
      const view = store('view', 'rows', text)
      let moved = 0
      watch(q, () => moved++)
      return { view, moved: () => moved }
    })
    value.view.value = 'grouped'
    await nextTick()
    expect(value.moved()).toBe(0)
    scope.stop()
  })
})

describe('useLogExplorer over the address', () => {
  it('reads a hostile window as the default one', () => {
    at('/?window=toString&from=0001-01-01T00:00:00Z&to=9999-01-01T00:00:00Z')
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: ['kind'], store: urlStore() }))
    expect(Date.parse(e.question.value.to) - Date.parse(e.question.value.from)).toBe(24 * HOUR)
    scope.stop()
  })
})

describe('useLogFetch', () => {
  it('asks nothing again for a write that leaves the question as it was', async () => {
    const { value, scope } = scoped(() => {
      const store = urlStore()
      const e = useLogExplorer({ fields: ['kind'], store })
      const view = store('view', 'rows', text)
      const c = counting()
      const answers = useLogFetch(e, c.fetchers)
      return { e, view, c, answers }
    })
    await vi.runAllTimersAsync()
    value.e.more(value.answers.next.value)
    await vi.runAllTimersAsync()
    expect(value.answers.rows.value).toHaveLength(2)
    const before = { ...value.c.asked }

    value.view.value = 'grouped'
    value.e.clear()
    await vi.runAllTimersAsync()
    expect(value.c.asked).toEqual(before)
    expect(value.answers.rows.value).toHaveLength(2)

    value.e.refresh()
    await vi.runAllTimersAsync()
    expect(value.c.asked.histogram).toBe(before.histogram + 1)
    scope.stop()
  })

  it('does not slide a zoomed window, live or not', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 20 })
      const c = counting()
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.zoom({ from: new Date(now - 3 * HOUR), to: new Date(now - HOUR) })
    await vi.runAllTimersAsync()
    const before = value.c.asked.histogram
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(200)
    expect(value.c.asked.histogram).toBe(before)
    scope.stop()
  })

  it('lets a slow server answer while live, and asks the newest question after', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 1_000 })
      const c = counting(2_500)
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.live.value = true
    // Two slides while the first answers are on their way: they land, and
    // the newest question is asked once, right after.
    await vi.advanceTimersByTimeAsync(2_600)
    expect(value.answers.buckets.value).toHaveLength(1)
    expect(value.c.asked.histogram).toBe(2)
    await vi.advanceTimersByTimeAsync(2_600)
    expect(value.c.asked.histogram).toBe(3)
    scope.stop()
  })

  it('asks again for a page that failed', async () => {
    let fail = true
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'] })
      const c = counting(0, async (_q, cursor) => {
        if (cursor && fail) throw new Error('503')
        return { rows: [{ id: cursor ?? 'first' }], next: cursor ? null : 'c1' }
      })
      return { e, answers: useLogFetch(e, c.fetchers) }
    })
    await vi.runAllTimersAsync()
    value.e.more(value.answers.next.value)
    await vi.runAllTimersAsync()
    expect(value.answers.error.value).toBeInstanceOf(Error)
    expect(value.e.cursors.value).toEqual([undefined])
    fail = false
    value.e.more(value.answers.next.value)
    await vi.runAllTimersAsync()
    expect(value.answers.rows.value.map((r) => r.id)).toEqual(['first', 'c1'])
    scope.stop()
  })

  it('draws the buckets on the step they answer until the next answer', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'] })
      const c = counting(1_000)
      const answers = useLogFetch(e, c.fetchers)
      const histogram = useHistogram(e, answers.buckets, undefined, answers.bucketsAnswer)
      return { e, answers, histogram }
    })
    const width = () => {
      const bar = value.histogram.geometry.value.bars[1]!
      return bar.end - bar.at
    }
    await vi.advanceTimersByTimeAsync(1_000)
    expect(width()).toBe(HOUR)
    value.e.choose('7d')
    await nextTick()
    expect(width()).toBe(HOUR)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(width()).toBe(6 * HOUR)
    scope.stop()
  })
})

describe('useLogFetch, round the edges', () => {
  it('reads no page after the last question’s pages while the new first page is on its way', async () => {
    const asked: [Record<string, string[]>, string | undefined][] = []
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'] })
      const c = counting(1_000, async (q, cursor) => {
        asked.push([q.fields, cursor])
        await new Promise((r) => setTimeout(r, 1_000))
        return {
          rows: [{ id: `${q.fields.kind ?? 'all'}:${cursor ?? 'first'}` }],
          next: q.fields.kind ? null : 'c1',
        }
      })
      return { e, answers: useLogFetch(e, c.fetchers) }
    })
    await vi.advanceTimersByTimeAsync(1_000)
    expect(value.answers.next.value).toBe('c1')
    value.e.toggle('kind', 'door')
    await nextTick()
    expect(value.answers.next.value).toBeNull()
    value.e.more('c1')
    await vi.advanceTimersByTimeAsync(2_000)
    expect(asked.some(([fields, cursor]) => fields.kind && cursor)).toBe(false)
    expect(value.answers.rows.value.map((r) => r.id)).toEqual(['door:first'])
    scope.stop()
  })

  it('reads on after a refresh of a window that does not slide', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'] })
      const c = counting()
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.zoom({ from: new Date(now - 3 * HOUR), to: new Date(now - HOUR) })
    await vi.runAllTimersAsync()
    value.e.more(value.answers.next.value)
    await vi.runAllTimersAsync()
    value.e.refresh()
    await vi.runAllTimersAsync()
    expect(value.answers.rows.value.map((r) => r.id)).toEqual(['first'])
    value.e.more(value.answers.next.value)
    await vi.runAllTimersAsync()
    expect(value.answers.rows.value.map((r) => r.id)).toEqual(['first', 'c1'])
    scope.stop()
  })

  it('asks a reader’s change at once while live, dropping the answer on its way', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 60_000 })
      const c = counting(5_000)
      c.fetchers.grouped = async (q) => {
        await new Promise((r) => setTimeout(r, 5_000))
        return [{ top: 'u1', value: q.fields.kind?.[0] ?? 'any', count: 1, first_at: null, last_at: null }]
      }
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(100)
    value.e.toggle('kind', 'door')
    await vi.advanceTimersByTimeAsync(5_100)
    expect(value.answers.groups.value.map((g) => g.value)).toEqual(['door'])
    scope.stop()
  })

  it('asks nothing once its scope is gone', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 1_000 })
      const c = counting(2_500)
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(1_100)
    scope.stop()
    const asked = value.c.asked.histogram
    await vi.advanceTimersByTimeAsync(10_000)
    expect(value.c.asked.histogram).toBe(asked)
  })

  it('clears a failure once its part is answered', async () => {
    let fail = true
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'] })
      const c = counting(0, async (_q, cursor) => {
        if (cursor && fail) throw new Error('503')
        return { rows: [{ id: cursor ?? 'first' }], next: cursor ? null : 'c1' }
      })
      return { e, answers: useLogFetch(e, c.fetchers) }
    })
    await vi.runAllTimersAsync()
    value.e.more(value.answers.next.value)
    await vi.runAllTimersAsync()
    expect(value.answers.error.value).toBeInstanceOf(Error)
    fail = false
    value.e.more(value.answers.next.value)
    await vi.runAllTimersAsync()
    expect(value.answers.error.value).toBeNull()
    scope.stop()
  })
})

describe('useLogFetch while a slide waits', () => {
  function live() {
    return scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 1_000 })
      const c = counting(0)
      // The histogram is slow; the first page lands at once.
      c.fetchers.histogram = (_q, step) => {
        c.asked.histogram++
        return new Promise((r) =>
          setTimeout(() => r([{ bucket: new Date(now).toISOString(), level: step.name, count: 1 }]), 2_500),
        )
      }
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
  }

  it('reads no page after the pages before the slide, and starts them again after', async () => {
    const { value, scope } = live()
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(10)
    expect(value.answers.next.value).toBe('c1')
    await vi.advanceTimersByTimeAsync(1_000)
    expect(value.answers.next.value).toBeNull()
    value.e.more('c1')
    expect(value.e.live.value).toBe(false)
    await vi.advanceTimersByTimeAsync(5_000)
    expect(value.answers.rows.value.map((r) => r.id)).toEqual(['first'])
    expect(value.e.cursors.value).toEqual([undefined])
    value.e.more(value.answers.next.value)
    await vi.runAllTimersAsync()
    expect(value.answers.rows.value.map((r) => r.id)).toEqual(['first', 'c1'])
    scope.stop()
  })

  it('asks a refresh once, not again when the waiting answers land', async () => {
    const { value, scope } = live()
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(1_100)
    // Live off asks the slid question at once; the refresh asks once more;
    // the answers that waited, landing, ask nothing again.
    value.e.live.value = false
    value.e.refresh()
    await vi.advanceTimersByTimeAsync(10_000)
    expect(value.c.asked.histogram).toBe(3)
    scope.stop()
  })

  it('does not wait on an answer aborted whose fetcher ignores its signal', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 1_000 })
      const c = counting(0)
      let first = true
      c.fetchers.histogram = () => {
        c.asked.histogram++
        // The first answer never comes, whatever its signal says.
        if (first) return ((first = false), new Promise(() => {}))
        return Promise.resolve([])
      }
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.live.value = true
    value.e.toggle('kind', 'door')
    await vi.advanceTimersByTimeAsync(3_100)
    expect(value.c.asked.histogram).toBeGreaterThanOrEqual(4)
    scope.stop()
  })
})

describe('useLogExplorer keys', () => {
  it('refuses two things under one key', () => {
    expect(() => scoped(() => useLogExplorer({ fields: ['to'] }))).toThrow(/two things under "to"/)
    expect(() => scoped(() => useLogExplorer({ fields: { to: { key: 'until' } } }))).not.toThrow()
  })

  it('reads a custom window the back button lands on without instants as the default', async () => {
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: [], store: urlStore() }))
    at('/?window=custom')
    window.dispatchEvent(new PopStateEvent('popstate'))
    await nextTick()
    expect(e.shown.value).toBe('24h')
    scope.stop()
  })
})

describe('round four', () => {
  it('zooms from one custom window to another, however far apart', () => {
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: [], longest: 30 * 24 * HOUR }))
    e.zoom({ from: new Date(now - 10 * HOUR), to: new Date(now - 9 * HOUR) })
    e.zoom({ from: new Date(now - 3 * HOUR), to: new Date(now - 2 * HOUR) })
    expect(e.preset.value).toBe('custom')
    expect(e.question.value.from).toBe(new Date(now - 3 * HOUR).toISOString())
    e.zoom({ from: new Date(now - 29 * 24 * HOUR), to: new Date(now - 28 * 24 * HOUR) })
    expect(e.preset.value).toBe('custom')
    expect(e.question.value.to).toBe(new Date(now - 28 * 24 * HOUR).toISOString())
    scope.stop()
  })

  it('gives up waiting on an answer that never comes, after two slides, then four, then eight', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 1_000 })
      const c = counting(0)
      c.fetchers.histogram = () => (c.asked.histogram++, new Promise(() => {}))
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.live.value = true
    // Asked at 0 s, then given up and asked again at 3 s, 8 s and 17 s.
    await vi.advanceTimersByTimeAsync(10_000)
    expect(value.c.asked.histogram).toBe(3)
    await vi.advanceTimersByTimeAsync(10_000)
    expect(value.c.asked.histogram).toBe(4)
    scope.stop()
  })

  it('asks a preset chosen again at once while live, not as a slide', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 60_000 })
      const c = counting(5_000)
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(1_000)
    const before = value.c.asked.histogram
    value.e.choose('24h')
    await nextTick()
    expect(value.c.asked.histogram).toBe(before + 1)
    scope.stop()
  })

  it('draws the histogram, the rows and the groups inside a root', async () => {
    const e = useLogExplorer({ fields: ['kind'] })
    const c = counting()
    const answers = useLogFetch(e, c.fetchers)
    const wrapper = mount(PartsHost, { props: { explorer: e, answers } })
    await vi.runAllTimersAsync()
    expect(wrapper.find('.bars').text()).toBe('1')
    expect(wrapper.find('.rows').text()).toBe('first')
    expect(wrapper.find('.groups').text()).toBe('0')
    await wrapper.find('button').trigger('click')
    await vi.runAllTimersAsync()
    expect(wrapper.find('.rows').text()).toBe('first,c1')
    expect(wrapper.find('button').exists()).toBe(false)
    wrapper.unmount()
  })
})

describe('round five', () => {
  it('shows a failure slower than a slide while live', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 1_000 })
      const c = counting(0)
      c.fetchers.histogram = () =>
        new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 1_500))
      const answers = useLogFetch(e, c.fetchers)
      const seen: unknown[] = []
      watch(answers.errors, (errors) => seen.push(errors.histogram))
      return { e, answers, seen }
    })
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(6_000)
    expect(value.seen.some((e) => e instanceof Error)).toBe(true)
    expect(value.answers.errors.value.histogram).toBeInstanceOf(Error)
    scope.stop()
  })

  it('freezes the window on screen when made custom, not instants left in the address', () => {
    at('/?from=2020-01-01T00:00:00Z&to=2020-01-02T00:00:00Z')
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: [], store: urlStore() }))
    expect(e.shown.value).toBe('24h')
    e.choose('custom')
    expect(e.question.value.to).toBe(new Date(now).toISOString())
    scope.stop()
  })

  it('ignores a preset it does not have', () => {
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: [] }))
    e.choose('2h')
    expect(e.preset.value).toBe('24h')
    scope.stop()
  })

  it('zooms by the plot a brush began on, whatever lands meanwhile', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: [] })
      const answered = shallowRef<{ range: { from: Date; to: Date }; step: Step } | null>(null)
      return { e, answered, h: useHistogram(e, [], undefined, answered) }
    })
    const plot = document.createElement('div')
    plot.getBoundingClientRect = () => ({ left: 0, width: 240 }) as DOMRect
    const event = (x: number) =>
      ({ button: 0, pointerId: 1, clientX: x, currentTarget: plot }) as unknown as PointerEvent
    value.h.handlers.onPointerdown(event(10))
    value.answered.value = {
      range: { from: new Date(now - 7 * 24 * HOUR), to: new Date(now) },
      step: { name: 'sixHours', ms: 6 * HOUR },
    }
    value.h.handlers.onPointermove(event(30))
    value.h.handlers.onPointerup(event(30))
    expect(Date.parse(value.e.from.value)).toBe(now - 23 * HOUR)
    expect(Date.parse(value.e.to.value)).toBe(now - 21 * HOUR)
    scope.stop()
  })
})

describe('round six', () => {
  it('slides a live window with the clock', async () => {
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: [], store: urlStore() }))
    e.live.value = true
    const before = e.question.value.to
    vi.advanceTimersByTime(10_000)
    await nextTick()
    expect(Date.parse(e.question.value.to) - Date.parse(before)).toBe(10_000)
    scope.stop()
  })

  it('takes a window a host writes piece by piece, in any order', async () => {
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: [] }))
    e.from.value = new Date(now - 3 * HOUR).toISOString()
    e.to.value = new Date(now - 2 * HOUR).toISOString()
    e.preset.value = 'custom'
    await nextTick()
    expect([e.preset.value, e.question.value.from]).toEqual([
      'custom',
      new Date(now - 3 * HOUR).toISOString(),
    ])
    e.preset.value = 'custom'
    e.from.value = new Date(now - 5 * HOUR).toISOString()
    e.to.value = new Date(now - 4 * HOUR).toISOString()
    await nextTick()
    expect([e.preset.value, e.question.value.to]).toEqual(['custom', new Date(now - 4 * HOUR).toISOString()])
    scope.stop()
  })

  it('waits longer for a live server slower than two slides, until it answers', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 1_000 })
      const c = counting(0)
      c.fetchers.histogram = (_q, step, signal) =>
        new Promise((resolve, reject) => {
          const t = setTimeout(
            () => resolve([{ bucket: new Date(now).toISOString(), level: step.name, count: 1 }]),
            3_200,
          )
          signal.addEventListener('abort', () => (clearTimeout(t), reject(signal.reason)))
        })
      return { e, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(20_000)
    expect(value.answers.buckets.value.length).toBeGreaterThan(0)
    scope.stop()
  })

  it('zooms to the last bar no further than the window', () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: [] })
      return { e, h: useHistogram(e, []) }
    })
    vi.setSystemTime(now + 30 * 60_000)
    value.e.refresh()
    const bars = value.h.geometry.value.bars
    value.h.zoomTo(bars[bars.length - 1]!)
    expect(value.e.to.value).toBe(new Date(now + 30 * 60_000).toISOString())
    scope.stop()
  })
})

describe('round seven', () => {
  it('keeps a window a date picker writes one instant per tick', async () => {
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: [] }))
    e.zoom({ from: new Date(now - 48 * HOUR), to: new Date(now - 24 * HOUR) })
    e.from.value = new Date(now - 5 * HOUR).toISOString()
    await nextTick()
    expect(e.shown.value).toBe('24h')
    e.to.value = new Date(now - 4 * HOUR).toISOString()
    await nextTick()
    expect(e.shown.value).toBe('custom')
    expect(e.question.value.from).toBe(new Date(now - 5 * HOUR).toISOString())
    scope.stop()
  })

  it('does not take a query change it did not see on an unrelated write', () => {
    const { value, scope } = scoped(() => {
      const store = urlStore()
      return { kind: store('kind', [], listOf()), q: store('q', '', text) }
    })
    window.history.pushState(null, '', '/?kind=door')
    value.q.value = 'x'
    expect(value.kind.value).toEqual([])
    expect(window.location.search).toBe('?kind=door&q=x')
    scope.stop()
  })
})

describe('round eight', () => {
  it('waits for a server steadily slower than two slides as long as it takes', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 1_000 })
      const c = counting(0)
      let landed = 0
      c.fetchers.histogram = (_q, step, signal) =>
        new Promise((resolve, reject) => {
          c.asked.histogram++
          const t = setTimeout(() => {
            landed++
            resolve([{ bucket: new Date(now).toISOString(), level: step.name, count: 1 }])
          }, 3_500)
          signal.addEventListener('abort', () => (clearTimeout(t), reject(signal.reason)))
        })
      return { e, c, answers: useLogFetch(e, c.fetchers), landed: () => landed }
    })
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(40_000)
    // After the first rounds settle, every ask lands: none is thrown away.
    expect(value.landed()).toBeGreaterThanOrEqual(value.c.asked.histogram - 2)
    scope.stop()
  })

  it('asks the slid question when live is turned off while it waits', async () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'], liveEvery: 1_000 })
      const c = counting(0)
      let first = true
      c.fetchers.histogram = () => {
        c.asked.histogram++
        if (first) return ((first = false), new Promise(() => {}))
        return Promise.resolve([])
      }
      return { e, c, answers: useLogFetch(e, c.fetchers) }
    })
    value.e.live.value = true
    await vi.advanceTimersByTimeAsync(1_100)
    value.e.live.value = false
    await vi.advanceTimersByTimeAsync(10)
    expect(value.c.asked.histogram).toBe(2)
    expect(value.answers.next.value).toBe('c1')
    scope.stop()
  })

  it('refuses an instant written without its offset, with either store', () => {
    for (const store of [undefined, urlStore()]) {
      const { value: e, scope } = scoped(() => useLogExplorer({ fields: [], store }))
      expect(() => (e.from.value = '2026-09-30T10:00')).toThrow(/RFC 3339/)
      e.from.value = '2026-09-30T10:00:00+02:00'
      expect(e.from.value).toBe('2026-09-30T08:00:00.000Z')
      e.from.value = ''
      expect(e.from.value).toBe('')
      scope.stop()
    }
  })

  it('hands a frozen value, so a change is written, never made in place', () => {
    const { value, scope } = scoped(() => {
      const store = urlStore()
      return { kind: store<string[]>('kind', [], listOf()) }
    })
    expect(() => value.kind.value.push('x')).toThrow(TypeError)
    expect(value.kind.value).toEqual([])
    value.kind.value = []
    expect(window.location.search).toBe('')
    scope.stop()
  })

  it('chooses one value alone, and clears them, in a facet', () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: ['kind'] })
      return { e, f: useFacet(e, [{ field: 'kind', value: 'door', count: 1 }], 'kind') }
    })
    value.e.toggle('kind', 'gps')
    value.f.only('door')
    expect(value.e.fields.kind.value).toEqual(['door'])
    value.f.clear()
    expect(value.e.fields.kind.value).toEqual([])
    scope.stop()
  })
})

describe('a window at its bounds', () => {
  it('keeps a brush past the longest window to the longest, ending where it ends', () => {
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: [], longest: 10 * HOUR }))
    e.zoom({ from: new Date(now - 11 * HOUR), to: new Date(now) })
    expect(e.preset.value).toBe('custom')
    expect(Date.parse(e.question.value.to) - Date.parse(e.question.value.from)).toBe(10 * HOUR)
    expect(e.question.value.to).toBe(new Date(now).toISOString())
    scope.stop()
  })

  it('reads a custom window without instants as the default one, and says so', () => {
    at('/?window=custom')
    const { value: e, scope } = scoped(() => useLogExplorer({ fields: [], store: urlStore() }))
    expect(e.shown.value).toBe('24h')
    expect(Date.parse(e.question.value.to) - Date.parse(e.question.value.from)).toBe(24 * HOUR)
    scope.stop()
  })
})

describe('useHistogram', () => {
  it('brushes across the plot and zooms to whole steps', () => {
    const { value, scope } = scoped(() => {
      const e = useLogExplorer({ fields: [], preset: '24h' })
      return { e, h: useHistogram(e, []) }
    })
    const plot = document.createElement('div')
    plot.getBoundingClientRect = () => ({ left: 100, width: 240 }) as DOMRect
    const event = (x: number) =>
      ({ button: 0, pointerId: 1, clientX: x, currentTarget: plot }) as unknown as PointerEvent
    // Ten pixels an hour: from 1 h into the window to 3.5 h, widened to
    // whole hours, 1 h to 4 h.
    value.h.handlers.onPointerdown(event(100 + 10))
    value.h.handlers.onPointermove(event(100 + 35))
    expect(value.h.brushed.value!.x).toBeCloseTo(1 / 24)
    expect(value.h.brushed.value!.width).toBeCloseTo(3 / 24)
    value.h.handlers.onPointerup(event(100 + 35))
    expect(value.e.preset.value).toBe('custom')
    expect(Date.parse(value.e.from.value)).toBe(now - 23 * HOUR)
    expect(Date.parse(value.e.to.value) - Date.parse(value.e.from.value)).toBe(3 * HOUR)
    expect(value.h.brushed.value).toBeNull()
    scope.stop()
  })
})

describe('the renderless parts', () => {
  it('follow the props they are given', async () => {
    const e = useLogExplorer({ fields: ['kind', 'zone'] })
    const c = counting()
    c.fetchers.facets = async () => [
      { field: 'kind', value: 'door', count: 3 },
      { field: 'zone', value: 'north', count: 7 },
    ]
    const answers = useLogFetch(e, c.fetchers)
    const wrapper = mount(FacetHost, { props: { explorer: e, answers, field: 'kind' } })
    await vi.runAllTimersAsync()
    expect(wrapper.text()).toBe('door')
    await wrapper.setProps({ field: 'zone' })
    expect(wrapper.text()).toBe('north')
    wrapper.unmount()
  })

  it('refuse a part drawn outside a root, by name', () => {
    expect(() => mount(LogRows, { slots: { default: () => 'x' } })).toThrow(
      /<LogRows> is drawn inside <LogExplorerRoot>/,
    )
  })
})

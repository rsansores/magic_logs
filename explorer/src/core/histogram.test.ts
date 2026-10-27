import { describe, expect, it } from 'vitest'
import { fill, geometry, niceMax, selection, stack, timeTicks } from './histogram.js'

const HOUR = 3_600_000
const at = (h: number) => Date.UTC(2026, 9, 1, h)
const range = { from: new Date(at(0)), to: new Date(at(4)) }

describe('stack', () => {
  it('turns one row per bucket and level into one bar per bucket, oldest first', () => {
    const bars = stack([
      { bucket: new Date(at(1)).toISOString(), level: 'error', count: 2 },
      { bucket: new Date(at(0)).toISOString(), level: 'info', count: 5 },
      { bucket: new Date(at(1)).toISOString(), level: 'info', count: 1 },
      { bucket: new Date(at(1)).toISOString(), level: null, count: 4 },
    ])
    expect(bars).toEqual([
      { at: at(0), counts: { info: 5 } },
      { at: at(1), counts: { error: 2, info: 1, '': 4 } },
    ])
  })
})

describe('fill', () => {
  it('keeps every bucket of the window, those without rows empty', () => {
    const filled = fill([{ at: at(2), counts: { info: 1 } }], range, HOUR)
    expect(filled.map((b) => b.at)).toEqual([at(0), at(1), at(2), at(3)])
    expect(filled[2]!.counts).toEqual({ info: 1 })
    expect(filled[0]!.counts).toEqual({})
  })

  it('begins at the bucket the window begins in, as time_bucket counts it', () => {
    const filled = fill([], { from: new Date(at(0) + 20 * 60_000), to: new Date(at(2)) }, HOUR)
    expect(filled.map((b) => b.at)).toEqual([at(0), at(1)])
  })
})

describe('geometry', () => {
  it('stacks each bar in the order of the levels, a level not among them last', () => {
    const { bars, max } = geometry([{ at: at(1), counts: { debug: 1, error: 2, info: 5 } }], range, HOUR, [
      'info',
      'error',
    ])
    expect(max).toBe(10)
    const bar = bars[1]!
    expect(bar.segments.map((s) => s.level)).toEqual(['info', 'error', 'debug'])
    expect(bar.segments.map((s) => [s.y0, s.y1])).toEqual([
      [0, 0.5],
      [0.5, 0.7],
      [0.7, 0.8],
    ])
    expect(bar.x).toBe(0.25)
    expect(bar.width).toBe(0.25)
    expect(bar.total).toBe(8)
  })

  it('draws an empty window with a scale of nothing', () => {
    const g = geometry([], range, HOUR)
    expect(g.max).toBe(0)
    expect(g.bars.every((b) => b.segments.length === 0)).toBe(true)
    expect(g.counts).toEqual([{ value: 0, at: 0 }])
  })

  it('clips a bucket that begins before the window to the plot', () => {
    const g = geometry([], { from: new Date(at(0) + HOUR / 2), to: new Date(at(2)) }, HOUR)
    expect(g.bars[0]!.x).toBe(0)
    expect(g.bars[0]!.width).toBeCloseTo(1 / 3)
  })
})

describe('the scale', () => {
  it('ticks in whole numbers for every top', () => {
    for (const top of [1, 2, 5, 10, 20, 50, 100, 200, 500]) {
      const { counts, max } = geometry([{ at: at(0), counts: { info: top } }], range, HOUR)
      expect(max).toBe(top)
      expect(counts.every((t) => Number.isInteger(t.value))).toBe(true)
      expect(counts.at(-1)!.value).toBe(top)
    }
  })

  it('takes a window of many thousands of bars without a stack overflow', () => {
    const long = { from: new Date(at(0)), to: new Date(at(0) + 200_000 * 60_000) }
    expect(geometry([], long, 60_000).bars).toHaveLength(200_000)
  })
})

describe('niceMax', () => {
  it('rounds up to 1, 2 or 5 × 10ⁿ', () => {
    expect([0, 1, 3, 7, 10, 11, 180, 4321].map(niceMax)).toEqual([0, 1, 5, 10, 10, 20, 200, 5000])
  })
})

describe('timeTicks', () => {
  it('places a handful of round instants across the window', () => {
    const ticks = timeTicks(range)
    expect(ticks.map((t) => t.value)).toEqual([at(0), at(1), at(2), at(3)])
    expect(ticks.map((t) => t.at)).toEqual([0, 0.25, 0.5, 0.75])
  })
})

describe('selection', () => {
  it('widens a brush to whole steps, in either direction', () => {
    expect(selection(range, 0.6, 0.3, HOUR)).toEqual({ from: new Date(at(1)), to: new Date(at(3)) })
  })

  it('selects one step for a press without a drag', () => {
    expect(selection(range, 0.3, 0.3, HOUR)).toEqual({ from: new Date(at(1)), to: new Date(at(2)) })
  })
})

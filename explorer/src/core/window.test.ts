import { describe, expect, it } from 'vitest'
import { defaultPresets, rangeOf, stepFor } from './window.js'

const HOUR = 3_600_000
const now = Date.UTC(2026, 9, 1, 12)

describe('rangeOf', () => {
  it('ends a named window now', () => {
    expect(rangeOf({ preset: '1h' }, now, defaultPresets, '24h')).toEqual({
      from: new Date(now - HOUR),
      to: new Date(now),
    })
  })

  it('reads a custom window from its two instants', () => {
    const from = '2026-09-01T00:00:00.000Z'
    const to = '2026-09-02T00:00:00.000Z'
    expect(rangeOf({ preset: 'custom', from, to }, now, defaultPresets, '24h')).toEqual({
      from: new Date(from),
      to: new Date(to),
    })
  })

  it('falls back on the default preset for a custom window that does not read', () => {
    const range = rangeOf(
      { preset: 'custom', from: '2026-09-02', to: '2026-09-01' },
      now,
      defaultPresets,
      '24h',
    )
    expect(range.to.getTime() - range.from.getTime()).toBe(24 * HOUR)
  })
})

describe('stepFor', () => {
  it('draws about a hundred bars at most, whole days past three weeks', () => {
    expect([HOUR, 6 * HOUR, 24 * HOUR, 7 * 24 * HOUR, 30 * 24 * HOUR].map((s) => stepFor(s).name)).toEqual([
      'minute',
      'fiveMinutes',
      'hour',
      'sixHours',
      'day',
    ])
  })
})

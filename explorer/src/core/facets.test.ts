import { describe, expect, it } from 'vitest'
import { facetOf, narrowed } from './facets.js'

describe('facetOf', () => {
  const rows = [
    { field: 'kind', value: 'door', count: 3 },
    { field: 'kind', value: 'battery', count: 9 },
    { field: 'kind', value: null, count: 4 },
    { field: 'zone', value: 'north', count: 12 },
  ]

  it("lists one field's values, largest first, without the rows that hold none", () => {
    expect(facetOf(rows, 'kind')).toEqual([
      { value: 'battery', count: 9, chosen: false },
      { value: 'door', count: 3, chosen: false },
    ])
  })

  it('keeps a chosen value no row holds, so it can be unchosen', () => {
    expect(facetOf(rows, 'kind', ['door', 'tamper']).map((i) => [i.value, i.count, i.chosen])).toEqual([
      ['battery', 9, false],
      ['door', 3, true],
      ['tamper', 0, true],
    ])
  })
})

describe('narrowed', () => {
  const window = { from: '2026-10-01T00:00:00Z', to: '2026-10-02T00:00:00Z' }
  it('is a chosen value or a search', () => {
    expect(narrowed({ ...window, fields: {} })).toBe(false)
    expect(narrowed({ ...window, fields: { kind: ['door'] } })).toBe(true)
    expect(narrowed({ ...window, fields: {}, search: 'gate' })).toBe(true)
  })
})

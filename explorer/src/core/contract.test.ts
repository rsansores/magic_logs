import { describe, expect, it } from 'vitest'
import answers from './contract/fixtures/answers.js'
import { facetOf } from './facets.js'
import { geometry, stack } from './histogram.js'
import { stepFor } from './window.js'

// The fixture is magic_logs' own answer, from a database, to one question;
// the type-check holds it to the generated row types (`satisfies`). Here,
// what the explorer works out from it.

const range = { from: new Date(answers.question.from), to: new Date(answers.question.to) }
const step = stepFor(range.to.getTime() - range.from.getTime())

describe('magic_logs’ answers', () => {
  it('stack into bars that count what the facets count', () => {
    const bars = stack(answers.histogram)
    const drawn = bars.reduce((n, b) => n + Object.values(b.counts).reduce((s, c) => s + c, 0), 0)
    const levels = facetOf(answers.facets, 'level').reduce((n, i) => n + i.count, 0)
    expect(drawn).toBeGreaterThan(0)
    expect(drawn).toBe(levels)
  })

  it('draw on the buckets the explorer lays out for the window', () => {
    const g = geometry(stack(answers.histogram), range, 6 * 3_600_000, ['Info', 'Warning', 'Error'])
    expect(g.bars).toHaveLength(4)
    expect(g.bars.every((b) => b.segments.length === 3)).toBe(true)
    expect(step.name).toBe('hour')
  })

  it('count a field under every filter but its own', () => {
    const kinds = facetOf(answers.facets, 'kind', answers.question.fields.kind)
    expect(kinds.map((k) => [k.value, k.chosen])).toEqual([
      ['door', true],
      ['power', true],
      ['gps', false],
    ])
  })

  it('group by top and value, largest first, with times that read', () => {
    const counts = answers.grouped.map((g) => g.count)
    expect(counts).toEqual([...counts].sort((a, b) => b - a))
    for (const g of answers.grouped) {
      expect(Number.isNaN(Date.parse(g.first_at ?? ''))).toBe(false)
      expect(Date.parse(g.first_at!)).toBeLessThanOrEqual(Date.parse(g.last_at!))
    }
  })
})

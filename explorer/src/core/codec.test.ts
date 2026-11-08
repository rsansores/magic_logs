import { describe, expect, it } from 'vitest'
import { between, flag, instant, listOf, oneOf } from './codec.js'

describe('codecs', () => {
  it('read only what they accept', () => {
    expect(listOf((v) => /^\d+$/.test(v)).parse('1,x,2,1')).toEqual(['1', '2'])
    expect(oneOf(['a', 'b']).parse('c')).toBeUndefined()
    expect(instant.parse('yesterday')).toBeUndefined()
    expect(instant.parse('2026-10-01T06:00:00+02:00')).toBe('2026-10-01T04:00:00.000Z')
    expect(flag.parse('1')).toBe(true)
    expect(flag.serialize(false)).toBe('0')
  })
})

describe('the codecs refuse what a server would', () => {
  it('reads an instant only as RFC 3339 with its offset, within years 1–9999', () => {
    expect(instant.parse('2026-10-01T06:00:00')).toBeUndefined()
    expect(instant.parse('1')).toBeUndefined()
    expect(instant.parse('+010000-01-01T00:00:00Z')).toBeUndefined()
    expect(instant.parse('2026-10-01T06:00Z')).toBe('2026-10-01T06:00:00.000Z')
    expect(instant.parse('2024-02-30T00:00:00Z')).toBeUndefined()
    expect(instant.parse('2024-04-31T00:00:00Z')).toBeUndefined()
    expect(instant.parse('2024-02-29T00:00:00Z')).toBe('2024-02-29T00:00:00.000Z')
    expect(instant.parse('0050-06-01T00:00:00Z')).toBe('0050-06-01T00:00:00.000Z')
    expect(instant.parse('2024-03-01T24:00:00Z')).toBeUndefined()
  })

  it('keeps a comma or a backslash inside a value', () => {
    const list = listOf()
    const values = ['a,b', 'c\\d', 'e']
    expect(list.serialize(values)).toBe('a\\,b,c\\\\d,e')
    expect(list.parse(list.serialize(values))).toEqual(values)
  })
})

describe('a range', () => {
  const range = between((b) => /^-?\d+(\.\d+)?$/.test(b))

  it('reads either bound, or both, and no range from neither', () => {
    expect(range.parse('1000..')).toEqual({ min: '1000' })
    expect(range.parse('..5000')).toEqual({ max: '5000' })
    expect(range.parse('1000.5..2000')).toEqual({ min: '1000.5', max: '2000' })
    expect(range.parse('..')).toBeUndefined()
    expect(range.parse('1000')).toBeUndefined()
    expect(range.parse('x..1')).toBeUndefined()
  })

  it('writes a range as it reads back, a dot that could join the two escaped', () => {
    const text = between()
    for (const bounds of [
      { min: '1000' },
      { max: 'F-12' },
      { min: 'a..b', max: 'c.' },
      { min: '.5', max: 'x\\y' },
    ])
      expect(text.parse(text.serialize(bounds))).toEqual(bounds)
    expect(text.serialize({ min: '1000.5', max: '2000' })).toBe('1000.5..2000')
    expect(text.serialize({ min: 'c.' })).toBe('c\\...')
  })
})

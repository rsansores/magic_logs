import { describe, expect, it } from 'vitest'
import { flag, instant, listOf, oneOf } from './codec.js'

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

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

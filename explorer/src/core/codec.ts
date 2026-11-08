// How one piece of the explorer's state is written as text and read back:
// what a URL parameter holds. A value that does not read is no value — an
// address can be typed or pasted, and one a server would refuse leaves the
// screen on an error nothing on it clears.

export interface Codec<T> {
  /** The value `raw` holds, or undefined when it holds none this reads. */
  parse(raw: string): T | undefined
  serialize(value: T): string
}

/** Text as it is. */
export const text: Codec<string> = {
  parse: (raw) => raw,
  serialize: (value) => value,
}

/** `1` or `true` is on; anything else, off. */
export const flag: Codec<boolean> = {
  parse: (raw) => raw === '1' || raw === 'true',
  serialize: (value) => (value ? '1' : '0'),
}

const RFC3339 = /^\d{4}-\d{2}-\d{2}T([01]\d|2[0-3]):[0-5]\d(:[0-5]\d(\.\d+)?)?(Z|[+-]\d{2}:\d{2})$/i

/** An instant, as RFC 3339 with its offset (a time without one would be
 * read in the browser's zone), written back in UTC. */
export const instant: Codec<string> = {
  parse: (raw) => {
    if (!RFC3339.test(raw)) return undefined
    const at = new Date(raw)
    if (Number.isNaN(at.getTime())) return undefined
    // A date that does not exist (February 30th) is not rolled into March.
    const [y, m, d] = raw.slice(0, 10).split('-').map(Number)
    // setUTCFullYear, not Date.UTC: years 0–99 are themselves.
    const day = new Date(0)
    day.setUTCFullYear(y!, m! - 1, d!)
    if (day.getUTCFullYear() !== y || day.getUTCMonth() !== m! - 1 || day.getUTCDate() !== d) return undefined
    const year = at.getUTCFullYear()
    return year < 1 || year > 9999 ? undefined : at.toISOString()
  },
  serialize: (value) => value,
}

/** One of `allowed`. */
export function oneOf<T extends string>(allowed: readonly T[]): Codec<T> {
  return {
    parse: (raw) => (allowed.includes(raw as T) ? (raw as T) : undefined),
    serialize: (value) => value,
  }
}

/** Comma-separated values, keeping those `accept` takes, each once. A comma
 * or a backslash inside a value is written after a backslash. */
export function listOf(accept: (value: string) => boolean = (v) => v.length > 0): Codec<string[]> {
  return {
    parse: (raw) => [...new Set(split(raw).filter(accept))],
    serialize: (value) => value.map((v) => v.replace(/[\\,]/g, (c) => `\\${c}`)).join(','),
  }
}

function split(raw: string): string[] {
  const out: string[] = []
  let value = ''
  for (let i = 0; i < raw.length; i++) {
    const c = raw[i]!
    if (c === '\\' && i + 1 < raw.length) value += raw[++i]
    else if (c === ',') {
      out.push(value)
      value = ''
    } else value += c
  }
  out.push(value)
  return out
}

/** A range's bounds, each inclusive and either absent: what a field between
 * two values holds, as magic_logs' `Condition::Between` reads it. */
export interface Bounds {
  min?: string
  max?: string
}

/** A range as `min..max`, either side left empty (`1000..`, `..2026-10-01`),
 * keeping the bounds `accept` takes. Neither is no range. A backslash, and a
 * dot that could be read as the `..` between the two, is written after a
 * backslash: `1000.5..2000` reads as it is. */
export function between(accept: (bound: string) => boolean = () => true): Codec<Bounds> {
  return {
    parse: (raw) => {
      const halves = cut(raw)
      if (!halves) return undefined
      const [min, max] = halves
      if ((min && !accept(min)) || (max && !accept(max))) return undefined
      const bounds: Bounds = { ...(min ? { min } : {}), ...(max ? { max } : {}) }
      return bounds.min === undefined && bounds.max === undefined ? undefined : bounds
    },
    serialize: (value) => `${escapeBound(value.min ?? '')}..${escapeBound(value.max ?? '')}`,
  }
}

/** The two sides of the first `..` no backslash escapes, unescaped. */
function cut(raw: string): [string, string] | undefined {
  let min = ''
  let max = ''
  let past = false
  for (let i = 0; i < raw.length; i++) {
    let c = raw[i]!
    if (c === '\\' && i + 1 < raw.length) c = raw[++i]!
    else if (!past && c === '.' && raw[i + 1] === '.') {
      past = true
      i++
      continue
    }
    if (past) max += c
    else min += c
  }
  return past ? [min, max] : undefined
}

function escapeBound(bound: string): string {
  let out = ''
  for (let i = 0; i < bound.length; i++) {
    const c = bound[i]!
    // A dot beside another, or at either end, could join the `..`.
    const loose =
      c === '.' && (i === 0 || i === bound.length - 1 || bound[i - 1] === '.' || bound[i + 1] === '.')
    out += c === '\\' || loose ? `\\${c}` : c
  }
  return out
}

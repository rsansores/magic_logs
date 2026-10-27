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

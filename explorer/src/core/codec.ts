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

/** An instant, as RFC 3339. */
export const instant: Codec<string> = {
  parse: (raw) => (Number.isNaN(Date.parse(raw)) ? undefined : new Date(raw).toISOString()),
  serialize: (value) => value,
}

/** One of `allowed`. */
export function oneOf<T extends string>(allowed: readonly T[]): Codec<T> {
  return {
    parse: (raw) => (allowed.includes(raw as T) ? (raw as T) : undefined),
    serialize: (value) => value,
  }
}

/** Comma-separated values, keeping those `accept` takes, each once. */
export function listOf(accept: (value: string) => boolean = (v) => v.length > 0): Codec<string[]> {
  return {
    parse: (raw) => [...new Set(raw.split(',').filter(accept))],
    serialize: (value) => value.join(','),
  }
}

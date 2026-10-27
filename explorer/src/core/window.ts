// The window an explorer looks at: a named one that slides with the clock,
// or two instants; and the step its histogram is drawn in.

/** The named windows, by how far back each looks, in ms. */
export type Presets = Record<string, number>

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR

export const defaultPresets = {
  '1h': HOUR,
  '24h': DAY,
  '7d': 7 * DAY,
  '30d': 30 * DAY,
} as const satisfies Presets

/** What the explorer looks at: a preset, or `from` and `to` (RFC 3339) when
 * the preset is `custom` and both are set. */
export interface Window {
  preset: string
  from?: string
  to?: string
}

export interface Range {
  from: Date
  to: Date
}

/** The window as two instants, a named one ending at `now`. A preset
 * `presets` does not name looks back as far as `fallback`. */
export function rangeOf(window: Window, now: number, presets: Presets, fallback: string): Range {
  if (window.preset === 'custom' && window.from && window.to) {
    const from = Date.parse(window.from)
    const to = Date.parse(window.to)
    if (!Number.isNaN(from) && !Number.isNaN(to) && from < to)
      return { from: new Date(from), to: new Date(to) }
  }
  const back = presets[window.preset] ?? presets[fallback] ?? DAY
  return { from: new Date(now - back), to: new Date(now) }
}

/** A histogram step: its name, for the host's request, and its span. A step
 * of whole hours or days is answered from magic_logs' counts. */
export interface Step {
  name: 'minute' | 'fiveMinutes' | 'hour' | 'sixHours' | 'day'
  ms: number
}

/** Each step, and the longest window it is used for: about a hundred bars
 * at most, and whole days past three weeks, where the daily counts answer. */
export const steps: readonly (Step & { upTo: number })[] = [
  { name: 'minute', ms: MINUTE, upTo: 2 * HOUR },
  { name: 'fiveMinutes', ms: 5 * MINUTE, upTo: 8 * HOUR },
  { name: 'hour', ms: HOUR, upTo: 4 * DAY },
  { name: 'sixHours', ms: 6 * HOUR, upTo: 20 * DAY },
  { name: 'day', ms: DAY, upTo: Infinity },
]

/** The step a window of `span` ms is drawn in. */
export function stepFor(span: number): Step {
  const { name, ms } = steps.find((s) => span <= s.upTo) ?? steps[steps.length - 1]!
  return { name, ms }
}

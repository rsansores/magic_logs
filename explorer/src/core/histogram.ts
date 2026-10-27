// A histogram's geometry, for any renderer: bars stacked by level, the
// scale's ticks and a brushed selection, each placed as a fraction of the
// plot (0 to 1), so an SVG, a canvas or a chart library draws it the same.

import type { Bucket, BucketRow } from './contract/types.js'
import type { Range } from './window.js'

/** `histogram`'s rows, one per bucket and level, as one bar per bucket. */
export function stack(rows: readonly BucketRow[]): Bucket[] {
  const byAt = new Map<number, Record<string, number>>()
  for (const row of rows) {
    const at = Date.parse(row.bucket)
    const counts = byAt.get(at) ?? {}
    const level = row.level ?? ''
    counts[level] = (counts[level] ?? 0) + row.count
    byAt.set(at, counts)
  }
  return [...byAt].sort(([a], [b]) => a - b).map(([at, counts]) => ({ at, counts }))
}

/** Every bucket of `range`, in steps of `step` ms counted from the epoch as
 * TimescaleDB's `time_bucket` counts them, those without rows empty. */
export function fill(buckets: readonly Bucket[], range: Range, step: number): Bucket[] {
  const byAt = new Map(buckets.map((b) => [b.at, b.counts]))
  const out: Bucket[] = []
  for (let at = Math.floor(range.from.getTime() / step) * step; at < range.to.getTime(); at += step)
    out.push({ at, counts: byAt.get(at) ?? {} })
  return out
}

/** One level's part of a bar, from `y0` to `y1` of the plot's height. */
export interface Segment {
  level: string
  count: number
  y0: number
  y1: number
}

export interface Bar {
  /** Where the bucket begins and ends, in ms. */
  at: number
  end: number
  /** Its left edge and width, as fractions of the plot's width. */
  x: number
  width: number
  total: number
  /** Bottom to top, in the order of the levels. */
  segments: Segment[]
}

export interface Tick {
  value: number
  /** Where it falls, as a fraction of the plot's width (time) or height (counts). */
  at: number
}

export interface Geometry {
  bars: Bar[]
  /** The top of the scale: the tallest bar, rounded up to 1, 2 or 5 × 10ⁿ. */
  max: number
  /** The scale's ticks, from 0 to `max`. */
  counts: Tick[]
  /** Round instants across the plot, a handful at most. */
  times: Tick[]
  /** What the plot spans: every bucket the window touches, whole, as the
   * counts count them. Fractions are of this; brush on it. */
  range: Range
}

/** The geometry of `buckets` over `range`, stacked in the order of `levels`;
 * a level not among them is stacked last. */
export function geometry(
  buckets: readonly Bucket[],
  range: Range,
  step: number,
  levels: readonly string[] = [],
): Geometry {
  // The plot spans the buckets the window touches, whole: magic_logs counts
  // each of them whole, the first and the last too.
  const from = Math.floor(range.from.getTime() / step) * step
  const to = Math.max(Math.ceil(range.to.getTime() / step) * step, from + step)
  const plot = { from: new Date(from), to: new Date(to) }
  const span = to - from
  const filled = fill(buckets, plot, step)
  const totals = filled.map((b) => Object.values(b.counts).reduce((sum, n) => sum + n, 0))
  const max = niceMax(totals.reduce((m, t) => Math.max(m, t), 0))
  const clip = (v: number) => Math.min(Math.max(v, 0), 1)
  const bars = filled.map((b, i): Bar => {
    const order = [
      ...levels,
      ...Object.keys(b.counts)
        .filter((l) => !levels.includes(l))
        .sort(),
    ]
    // Summed in counts and divided once, so a stack's edges meet exactly.
    let below = 0
    const segments: Segment[] = []
    for (const level of order) {
      const count = b.counts[level] ?? 0
      if (count <= 0) continue
      segments.push({ level, count, y0: below / max, y1: (below + count) / max })
      below += count
    }
    const x = clip((b.at - from) / span)
    return {
      at: b.at,
      end: b.at + step,
      x,
      width: clip((b.at + step - from) / span) - x,
      total: totals[i]!,
      segments,
    }
  })
  return { bars, max, counts: countTicks(max), times: timeTicks(plot), range: plot }
}

/** `n` rounded up to 1, 2 or 5 × 10ⁿ; 0 stays 0. */
export function niceMax(n: number): number {
  if (n <= 0) return 0
  const power = 10 ** Math.floor(Math.log10(n))
  return ([1, 2, 5, 10].find((m) => m * power >= n) ?? 10) * power
}

function countTicks(max: number): Tick[] {
  if (max === 0) return [{ value: 0, at: 0 }]
  // 1 and 5 × 10ⁿ split into fifths of whole numbers (from 5), 2 × 10ⁿ into
  // halves; 1 and 2 themselves into ones.
  const parts = max >= 5 && /^[15]/.test(String(max)) ? 5 : max <= 2 ? max : 2
  return Array.from({ length: parts + 1 }, (_, i) => ({ value: (max / parts) * i, at: i / parts }))
}

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR
const INTERVALS = [
  MINUTE,
  5 * MINUTE,
  15 * MINUTE,
  30 * MINUTE,
  HOUR,
  3 * HOUR,
  6 * HOUR,
  12 * HOUR,
  DAY,
  2 * DAY,
  7 * DAY,
  14 * DAY,
]

/** Round instants of `range`, at most `most` of them: whole minutes, hours
 * or days, counted from the epoch (UTC). */
export function timeTicks(range: Range, most = 6): Tick[] {
  const from = range.from.getTime()
  const span = Math.max(range.to.getTime() - from, 1)
  const every = INTERVALS.find((i) => span / i <= most) ?? Math.ceil(span / most / DAY) * DAY
  const out: Tick[] = []
  for (let t = Math.ceil(from / every) * every; t < range.to.getTime(); t += every)
    out.push({ value: t, at: (t - from) / span })
  return out
}

/** The window a brush across `a` and `b` (fractions of the plot's width, in
 * either order) selects, widened to whole steps and at least one, ending
 * no later than the window. */
export function selection(range: Range, a: number, b: number, step: number): Range {
  const from = range.from.getTime()
  const span = range.to.getTime() - from
  const at = (f: number) => from + Math.min(Math.max(f, 0), 1) * span
  // Whole steps, but no further than the window: not into the future.
  const end = Math.min(
    Math.max(
      Math.ceil(at(Math.max(a, b)) / step) * step,
      Math.floor(at(Math.min(a, b)) / step) * step + step,
    ),
    range.to.getTime(),
  )
  // A press at the window's very end selects the bucket it ends in.
  const first = Math.floor(at(Math.min(a, b)) / step) * step
  const start = first < end ? first : Math.floor((end - 1) / step) * step
  return { from: new Date(start), to: new Date(end) }
}

// A made-up fleet's events, and a server that answers magic_logs' four
// questions over them in the shapes the crate's rows serialize to — what a
// host's handlers would return from `page`, `histogram`, `facets` and
// `grouped`.

import type { BucketRow, FacetCountRow, GroupRow, Page, Question, Step } from '../src/index.js'

export interface Event {
  id: string
  at: number
  level: Level
  kind: string
  unit: string
  zone: string
  message: string
}

export const levels = ['info', 'warning', 'error', 'critical'] as const
export type Level = (typeof levels)[number]
export const fields = ['level', 'kind', 'zone', 'unit'] as const

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR

/** Deterministic: the demo looks the same on every load. */
function random(seed: number) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

const kinds: { kind: string; level: Level; weight: number; says: (unit: string) => string }[] = [
  { kind: 'engine_start', level: 'info', weight: 30, says: (u) => `${u} started its engine` },
  { kind: 'door_opened', level: 'info', weight: 24, says: (u) => `Cargo door opened on ${u}` },
  { kind: 'geofence_exit', level: 'warning', weight: 9, says: (u) => `${u} left its assigned area` },
  { kind: 'low_battery', level: 'warning', weight: 8, says: (u) => `${u} battery at 18 %` },
  { kind: 'gps_lost', level: 'warning', weight: 6, says: (u) => `${u} lost its GPS fix` },
  { kind: 'temperature_high', level: 'error', weight: 3, says: (u) => `Cold chain above 8 °C on ${u}` },
  { kind: 'sync_failed', level: 'error', weight: 2, says: (u) => `${u} could not upload its log` },
  { kind: 'tamper', level: 'critical', weight: 0.4, says: (u) => `Seal broken on ${u}` },
]
const zones = ['North', 'South', 'East', 'West', 'Harbor']
const units = Array.from({ length: 36 }, (_, i) => `truck-${String(i + 1).padStart(2, '0')}`)

/** Thirty days of a fleet's events, busier by day than by night, and an
 * incident in the harbor an hour and a half ago. */
export function fleet(now: number): Event[] {
  const rand = random(42)
  const pick = <T>(xs: readonly T[]) => xs[Math.floor(rand() * xs.length)]!
  const total = kinds.reduce((s, k) => s + k.weight, 0)
  const kindOf = () => {
    let r = rand() * total
    return kinds.find((k) => (r -= k.weight) < 0) ?? kinds[0]!
  }
  const events: Event[] = []
  const push = (at: number, k: (typeof kinds)[number], unit: string, zone: string) =>
    events.push({
      id: `e${events.length}`,
      at,
      level: k.level,
      kind: k.kind,
      unit,
      zone,
      message: k.says(unit),
    })

  const start = now - 30 * DAY
  for (let at = start; at < now; at += 2 * MINUTE) {
    const hour = new Date(at).getUTCHours()
    const busy = 0.35 + 0.65 * Math.max(0, Math.sin(((hour - 5) / 24) * 2 * Math.PI))
    const n = Math.floor(rand() * 7 * busy)
    for (let i = 0; i < n; i++) {
      const unit = pick(units)
      push(at + Math.floor(rand() * 2 * MINUTE), kindOf(), unit, zones[units.indexOf(unit) % zones.length]!)
    }
  }
  const incident = now - 90 * MINUTE
  const bad = kinds.filter(
    (k) => k.kind === 'temperature_high' || k.kind === 'sync_failed' || k.kind === 'tamper',
  )
  for (let i = 0; i < 260; i++) {
    const unit = units[4 + 5 * Math.floor(rand() * 6)]!
    push(incident + Math.floor(rand() * rand() * 40 * MINUTE), pick(bad), unit, 'Harbor')
  }
  return events.sort((a, b) => b.at - a.at || (a.id < b.id ? 1 : -1))
}

type Field = (typeof fields)[number]

function matches(e: Event, q: Question, except?: string) {
  if (e.at < Date.parse(q.from) || e.at >= Date.parse(q.to)) return false
  for (const [field, values] of Object.entries(q.fields))
    if (field !== except && values.length && !values.includes(e[field as Field])) return false
  if (q.search) {
    const s = q.search.toLowerCase()
    if (!e.message.toLowerCase().includes(s) && !e.unit.includes(s)) return false
  }
  return true
}

const later = <T>(answer: T, signal: AbortSignal) =>
  new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => resolve(answer), 60)
    signal.addEventListener('abort', () => (clearTimeout(timer), reject(signal.reason)))
  })

export function server(events: Event[]) {
  return {
    histogram(q: Question, step: Step, signal: AbortSignal): Promise<BucketRow[]> {
      // As magic_logs' counts do: the bucket the window begins in, whole.
      const whole = { ...q, from: new Date(Math.floor(Date.parse(q.from) / step.ms) * step.ms).toISOString() }
      const counts = new Map<string, number>()
      for (const e of events) {
        if (!matches(e, whole)) continue
        const key = `${Math.floor(e.at / step.ms) * step.ms}|${e.level}`
        counts.set(key, (counts.get(key) ?? 0) + 1)
      }
      const rows = [...counts].map(([key, count]) => {
        const [at, level] = key.split('|')
        return { bucket: new Date(Number(at)).toISOString(), level: level!, count }
      })
      return later(rows, signal)
    },

    facets(q: Question, names: readonly string[], signal: AbortSignal): Promise<FacetCountRow[]> {
      const rows: FacetCountRow[] = []
      for (const field of names) {
        const counts = new Map<string, number>()
        for (const e of events)
          if (matches(e, q, field)) counts.set(e[field as Field], (counts.get(e[field as Field]) ?? 0) + 1)
        for (const [value, count] of counts) rows.push({ field, value, count })
      }
      return later(rows, signal)
    },

    page(q: Question, cursor: string | undefined, signal: AbortSignal): Promise<Page<Event>> {
      const [at, id] = cursor ? [Number(cursor.split(':')[0]), cursor.split(':')[1]!] : [Infinity, '']
      const rows: Event[] = []
      for (const e of events) {
        if (e.at > at || (e.at === at && e.id >= id) || !matches(e, q)) continue
        rows.push(e)
        if (rows.length === 50) break
      }
      const last = rows[rows.length - 1]
      return later({ rows, next: rows.length === 50 && last ? `${last.at}:${last.id}` : null }, signal)
    },

    grouped(q: Question, signal: AbortSignal): Promise<GroupRow[]> {
      const groups = new Map<string, { count: number; first: number; last: number }>()
      for (const e of events) {
        if (!matches(e, q)) continue
        const key = `${e.unit}|${e.kind}`
        const g = groups.get(key) ?? { count: 0, first: e.at, last: e.at }
        g.count++
        g.first = Math.min(g.first, e.at)
        g.last = Math.max(g.last, e.at)
        groups.set(key, g)
      }
      const rows = [...groups]
        .map(([key, g]) => {
          const [top, value] = key.split('|')
          return {
            top: top!,
            value: value!,
            count: g.count,
            first_at: new Date(g.first).toISOString(),
            last_at: new Date(g.last).toISOString(),
          }
        })
        .sort((a, b) => b.count - a.count)
        .slice(0, 50)
      return later(rows, signal)
    },
  }
}

// The fleet example's server (examples/fleet), asked through the dev
// server's proxy: what a host's fetch functions look like.

import type { BucketRow, FacetCountRow, GroupRow, Page, Question, Step } from '../src/index.js'
import type { Event } from './server.js'

function query(q: Question, extra: Record<string, string | undefined> = {}) {
  const params = new URLSearchParams({ from: q.from, to: q.to })
  for (const [field, values] of Object.entries(q.fields)) params.set(field, values.join(','))
  if (q.search) params.set('search', q.search)
  for (const [key, value] of Object.entries(extra)) if (value) params.set(key, value)
  return params
}

async function get<T>(path: string, params: URLSearchParams, signal: AbortSignal): Promise<T> {
  const answer = await fetch(`${path}?${params}`, { signal })
  if (!answer.ok) throw new Error(`${answer.status} ${await answer.text()}`)
  return (await answer.json()) as T
}

export const fleetServer = {
  histogram: (q: Question, step: Step, signal: AbortSignal) =>
    get<BucketRow[]>('/api/histogram', query(q, { step: step.name }), signal),
  facets: (q: Question, _fields: readonly string[], signal: AbortSignal) =>
    get<FacetCountRow[]>('/api/facets', query(q), signal),
  page: async (q: Question, cursor: string | undefined, signal: AbortSignal): Promise<Page<Event>> => {
    const page = await get<{ rows: (Omit<Event, 'at'> & { at: string })[]; next: string | null }>(
      '/api/page',
      query(q, { cursor }),
      signal,
    )
    return { rows: page.rows.map((r) => ({ ...r, at: Date.parse(r.at) })), next: page.next }
  },
  grouped: (q: Question, signal: AbortSignal) => get<GroupRow[]>('/api/grouped', query(q), signal),
}

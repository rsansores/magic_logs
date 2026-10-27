// A facet's values as a list to choose from: counted, chosen, largest first.

import type { FacetCountRow, Question } from './contract/types.js'

export interface FacetItem {
  value: string
  count: number
  chosen: boolean
}

/** The values of `field` among `facets`' rows, largest first; a chosen value
 * no row holds is listed with 0, so it can be unchosen. Rows without a
 * value, or with an empty one, are not listed: no filter names them. */
export function facetOf(
  rows: readonly FacetCountRow[],
  field: string,
  chosen: readonly string[] = [],
): FacetItem[] {
  const counts = new Map<string, number>()
  for (const row of rows)
    if (row.field === field && row.value != null && row.value !== '')
      counts.set(row.value, (counts.get(row.value) ?? 0) + row.count)
  for (const value of chosen) if (!counts.has(value)) counts.set(value, 0)
  return [...counts]
    .map(([value, count]) => ({ value, count, chosen: chosen.includes(value) }))
    .sort((a, b) => b.count - a.count || a.value.localeCompare(b.value))
}

/** Whether `question` narrows its window: a value chosen, or a search. */
export function narrowed(question: Question): boolean {
  return Object.values(question.fields).some((v) => v.length > 0) || !!question.search
}

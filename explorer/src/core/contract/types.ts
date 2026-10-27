// The answers of magic_logs' four questions, as its rows serialize with the
// `serde` feature (`Bucket`, `FacetCount`, `Group`), and the question every
// one of them is asked.

/** One bar of the histogram, one level of it: `histogram`'s rows. */
export interface BucketRow {
  /** Where the bucket begins, an RFC 3339 instant. */
  bucket: string
  /** The level, as text; null for rows without one, or a table with no level. */
  level: string | null
  count: number
}

/** How many rows hold `value` in `field` under every other field's filter:
 * `facets`' rows. */
export interface FacetCountRow {
  field: string
  value: string | null
  count: number
}

/** One `top` and one value of the grouped field: `grouped`'s rows. */
export interface GroupRow {
  top: string | null
  value: string | null
  count: number
  first_at: string | null
  last_at: string | null
}

/** What every question shares: the window, each field's chosen values (any
 * of them; every field must match) and the search. A host turns it into its
 * request; its server turns that into a `magic_logs::Filter`. */
export interface Question {
  /** RFC 3339, inclusive. */
  from: string
  /** RFC 3339, exclusive. */
  to: string
  /** Only fields with a value chosen are named. */
  fields: Record<string, string[]>
  search?: string
}

/** One bar, stacked: how many rows of each level began in it. */
export interface Bucket {
  /** Where the bucket begins, in ms since the epoch. */
  at: number
  counts: Record<string, number>
}

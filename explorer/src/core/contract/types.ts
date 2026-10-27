// The answers of magic_logs' questions, as its rows serialize with the
// `serde` feature (generated, in rows.ts), and the question every one of
// them is asked.

export type { BucketRow, FacetCountRow, GroupRow } from './rows.js'

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

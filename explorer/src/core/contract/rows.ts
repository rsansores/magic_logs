// Generated from magic_logs' answer rows by its test `the_explorers_types_are_generated_from_these`: UPDATE_CONTRACT=1 rewrites it.

/**
 * One bar of the histogram: the rows of `level` in the bucket that begins
 * at `bucket`.
 */
export type BucketRow = { 
/**
 * Where the bucket begins.
 */
bucket: string, 
/**
 * The level, as text; `None` for rows without one, or a table with no level.
 */
level: string | null, 
/**
 * How many rows.
 */
count: number, };

/**
 * How many rows hold `value` in `field`, under every other field's filter.
 */
export type FacetCountRow = { 
/**
 * The field counted.
 */
field: string, 
/**
 * The value, as text; `None` for the rows that hold none.
 */
value: string | null, 
/**
 * How many rows hold it.
 */
count: number, };

/**
 * One `top` and one value of the grouped field: how many rows, the first
 * and the last.
 */
export type GroupRow = { 
/**
 * The `top`, as text.
 */
top: string | null, 
/**
 * The grouped field's value, as text.
 */
value: string | null, 
/**
 * How many rows.
 */
count: number, 
/**
 * The time of the group's first row.
 */
first_at: string | null, 
/**
 * The time of its last.
 */
last_at: string | null, };

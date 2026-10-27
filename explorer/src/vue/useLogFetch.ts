// The four questions asked with the host's fetch functions, for a host with
// no data layer of its own; one with vue-query (or anything like it) asks
// them its own way from `explorer.question`, `explorer.step` and
// `explorer.cursors`, and hands the answers to the parts the same way.
//
// Each answer is kept until the next one arrives, so a changed question
// does not empty the screen; an answer to a question no longer asked is
// dropped.

import { computed, getCurrentScope, onScopeDispose, ref, shallowRef, watch, type Ref } from 'vue'
import type { Bucket, BucketRow, FacetCountRow, GroupRow, Question } from '../core/contract/types.js'
import { stack } from '../core/histogram.js'
import type { Step } from '../core/window.js'
import type { LogExplorer } from './useLogExplorer.js'

/** A page of rows, newest first, and where the next one begins. */
export interface Page<Row> {
  rows: Row[]
  next: string | null
}

export interface LogFetchers<Row> {
  histogram?: (question: Question, step: Step, signal: AbortSignal) => Promise<BucketRow[]>
  facets?: (question: Question, fields: readonly string[], signal: AbortSignal) => Promise<FacetCountRow[]>
  page?: (question: Question, cursor: string | undefined, signal: AbortSignal) => Promise<Page<Row>>
  grouped?: (question: Question, signal: AbortSignal) => Promise<GroupRow[]>
}

/** What the parts draw: the answers the host holds now. */
export interface LogAnswers<Row> {
  buckets: Readonly<Ref<Bucket[]>>
  facets: Readonly<Ref<FacetCountRow[]>>
  rows: Readonly<Ref<Row[]>>
  /** Where the page after the last one read begins; null after the last. */
  next: Readonly<Ref<string | null>>
  groups: Readonly<Ref<GroupRow[]>>
  /** Which questions wait for an answer. */
  loading: Readonly<Ref<{ histogram: boolean; facets: boolean; rows: boolean; groups: boolean }>>
  /** The last failure of the question asked now, when one failed. */
  error: Readonly<Ref<unknown>>
}

export function useLogFetch<Row>(explorer: LogExplorer, fetchers: LogFetchers<Row>): LogAnswers<Row> {
  const buckets = shallowRef<Bucket[]>([])
  const facets = shallowRef<FacetCountRow[]>([])
  const pages = shallowRef<Page<Row>[]>([])
  const groups = shallowRef<GroupRow[]>([])
  const loading = ref({ histogram: false, facets: false, rows: false, groups: false })
  const error = shallowRef<unknown>(null)

  let asked = new AbortController()

  async function ask<T>(
    which: keyof typeof loading.value,
    fetch: ((signal: AbortSignal) => Promise<T>) | undefined,
    keep: (answer: T) => void,
  ) {
    if (!fetch) return
    const { signal } = asked
    loading.value[which] = true
    try {
      const answer = await fetch(signal)
      if (!signal.aborted) keep(answer)
    } catch (e) {
      if (!signal.aborted) error.value = e
    } finally {
      if (!signal.aborted) loading.value[which] = false
    }
  }

  watch(
    explorer.question,
    (question) => {
      asked.abort()
      asked = new AbortController()
      error.value = null
      loading.value = { histogram: false, facets: false, rows: false, groups: false }
      const step = explorer.step.value
      void ask('histogram', fetchers.histogram && ((s) => fetchers.histogram!(question, step, s)), (rows) => {
        buckets.value = stack(rows)
      })
      void ask(
        'facets',
        fetchers.facets && ((s) => fetchers.facets!(question, explorer.fieldNames, s)),
        (rows) => {
          facets.value = rows
        },
      )
      void ask('rows', fetchers.page && ((s) => fetchers.page!(question, undefined, s)), (page) => {
        pages.value = [page]
      })
      void ask('groups', fetchers.grouped && ((s) => fetchers.grouped!(question, s)), (rows) => {
        groups.value = rows
      })
    },
    { immediate: true },
  )

  // A cursor added after the first: the page it begins, appended.
  watch(explorer.cursors, (cursors, before) => {
    const cursor = cursors[cursors.length - 1]
    if (cursors.length <= 1 || cursors.length <= (before?.length ?? 0)) return
    const question = explorer.question.value
    void ask('rows', fetchers.page && ((s) => fetchers.page!(question, cursor, s)), (page) => {
      pages.value = [...pages.value, page]
    })
  })

  if (getCurrentScope()) onScopeDispose(() => asked.abort())

  return {
    buckets,
    facets,
    rows: computed(() => pages.value.flatMap((p) => p.rows)),
    next: computed(() => pages.value[pages.value.length - 1]?.next ?? null),
    groups,
    loading,
    error,
  }
}

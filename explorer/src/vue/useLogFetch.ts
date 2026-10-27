// The four questions asked with the host's fetch functions, for a host with
// no data layer of its own; one with vue-query (or anything like it) asks
// them its own way from `explorer.question`, `explorer.step`,
// `explorer.cursors` and `explorer.asked`, and hands the answers to the
// parts the same way.
//
// Each answer is kept until the next one arrives, so a changed question
// does not empty the screen; an answer to a question no longer asked is
// dropped. A live window that slides while answers are on their way waits
// for them, and asks once more after: a server slower than the slide still
// answers — within two slides, after which the wait is given up.

import { computed, getCurrentScope, onScopeDispose, ref, shallowRef, watch, type Ref } from 'vue'
import type { Bucket, BucketRow, FacetCountRow, GroupRow, Question } from '../core/contract/types.js'
import { stack } from '../core/histogram.js'
import type { Range, Step } from '../core/window.js'
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

/** The window and step a histogram's buckets answer: the bars are drawn on
 * these until the next answer arrives, not on the window asked meanwhile. */
export interface Answered {
  range: Range
  step: Step
}

type Which = 'histogram' | 'facets' | 'rows' | 'groups'

/** What the parts draw: the answers the host holds now. */
export interface LogAnswers<Row> {
  buckets: Readonly<Ref<Bucket[]>>
  /** What `buckets` answer; null before the first answer. */
  bucketsAnswer: Readonly<Ref<Answered | null>>
  facets: Readonly<Ref<FacetCountRow[]>>
  rows: Readonly<Ref<Row[]>>
  /** Where the page after the last one read begins; null after the last. */
  next: Readonly<Ref<string | null>>
  groups: Readonly<Ref<GroupRow[]>>
  /** Which questions wait for an answer. */
  loading: Readonly<Ref<Record<Which, boolean>>>
  /** Each part's failure of the question asked now, until it is answered. */
  errors: Readonly<Ref<Record<Which, unknown>>>
  /** The first of `errors`, whichever part it is. */
  error: Readonly<Ref<unknown>>
}

export function useLogFetch<Row>(explorer: LogExplorer, fetchers: LogFetchers<Row>): LogAnswers<Row> {
  const buckets = shallowRef<Bucket[]>([])
  const bucketsAnswer = shallowRef<Answered | null>(null)
  const facets = shallowRef<FacetCountRow[]>([])
  const pages = shallowRef<Page<Row>[]>([])
  const groups = shallowRef<GroupRow[]>([])
  const loading = ref<Record<Which, boolean>>({ histogram: false, facets: false, rows: false, groups: false })
  const errors = shallowRef<Record<Which, unknown>>({
    histogram: null,
    facets: null,
    rows: null,
    groups: null,
  })

  let asked = new AbortController()
  let inFlight = 0
  let again = false
  let waitingSince: number | null = null
  let disposed = false
  // Which asking the pages answer: until its first page lands, the pages on
  // screen are the last question's, and no page may be read after them.
  let asking = 0
  const pagesOf = ref(0)
  const current = ref(0)

  async function ask<T>(
    which: Which,
    fetch: ((signal: AbortSignal) => Promise<T>) | undefined,
    keep: (answer: T) => void,
    failed?: () => void,
  ) {
    if (!fetch) return
    const { signal } = asked
    loading.value[which] = true
    // Only what the question asked now waits on: an answer aborted, whose
    // fetcher ignores its signal, holds nothing up.
    inFlight++
    try {
      const answer = await fetch(signal)
      if (!signal.aborted) {
        keep(answer)
        errors.value = { ...errors.value, [which]: null }
      }
    } catch (e) {
      if (!signal.aborted) {
        errors.value = { ...errors.value, [which]: e }
        failed?.()
      }
    } finally {
      if (!signal.aborted) {
        inFlight--
        loading.value[which] = false
      }
      if (inFlight === 0 && again && !disposed) {
        again = false
        askAll()
      }
    }
  }

  /** Asks the four questions; `anew` when the reader changed the question,
   * whose failures before are not this question's. A slide or a refresh
   * keeps them, until each part is answered. */
  function askAll(anew = false) {
    if (disposed) return
    again = false
    waitingSince = null
    inFlight = 0
    asked.abort()
    asked = new AbortController()
    // The pages start again from the newest, whatever was read meanwhile.
    if (explorer.cursors.value.length > 1) explorer.cursors.value = [undefined]
    if (anew) errors.value = { histogram: null, facets: null, rows: null, groups: null }
    loading.value = { histogram: false, facets: false, rows: false, groups: false }
    const question = explorer.question.value
    const answered = { range: explorer.range.value, step: explorer.step.value }
    const these = ++asking
    current.value = these
    void ask(
      'histogram',
      fetchers.histogram && ((s) => fetchers.histogram!(question, answered.step, s)),
      (rows) => {
        buckets.value = stack(rows)
        bucketsAnswer.value = answered
      },
    )
    void ask(
      'facets',
      fetchers.facets && ((s) => fetchers.facets!(question, explorer.fieldNames, s)),
      (rows) => {
        facets.value = rows
      },
    )
    void ask('rows', fetchers.page && ((s) => fetchers.page!(question, undefined, s)), (page) => {
      pages.value = [page]
      pagesOf.value = these
    })
    void ask('groups', fetchers.grouped && ((s) => fetchers.grouped!(question, s)), (rows) => {
      groups.value = rows
    })
  }

  watch(
    [explorer.question, explorer.asked, explorer.slides],
    ([question, askedNow, slidesNow], [before, askedBefore, slidesBefore]) => {
      // The clock slid a live window while answers are on their way: they
      // land, and the newest question is asked after them — unless they
      // have kept it waiting two slides already, when they are dropped.
      // Anything the reader changed is asked at once.
      if (question === before && askedNow === askedBefore) return
      const slid =
        slidesNow !== slidesBefore &&
        askedNow === askedBefore &&
        before !== undefined &&
        slide(before, question)
      const waited = waitingSince !== null && Date.now() - waitingSince >= 2 * explorer.liveEvery
      if (slid && inFlight > 0 && !waited) {
        // The pages on screen answer the window before the slide: none is
        // read after them until the slid question's first page lands.
        again = true
        waitingSince ??= Date.now()
        current.value = ++asking
      } else askAll(!slid && askedNow === askedBefore)
    },
    { immediate: true },
  )

  // A cursor added after the first: the page it begins, appended — when it
  // follows the last page read for the question asked now. A page that
  // fails gives its cursor back, so asking for it again asks again.
  watch(explorer.cursors, (cursors, before) => {
    const cursor = cursors[cursors.length - 1]
    if (cursors.length <= 1 || cursors.length <= (before?.length ?? 0)) return
    const giveBack = () => {
      if (explorer.cursors.value[explorer.cursors.value.length - 1] === cursor)
        explorer.cursors.value = explorer.cursors.value.slice(0, -1)
    }
    const these = current.value
    if (pagesOf.value !== these || pages.value[pages.value.length - 1]?.next !== cursor) return giveBack()
    const question = explorer.question.value
    void ask(
      'rows',
      fetchers.page && ((s) => fetchers.page!(question, cursor, s)),
      (page) => {
        if (pagesOf.value === these && pages.value[pages.value.length - 1]?.next === cursor)
          pages.value = [...pages.value, page]
      },
      giveBack,
    )
  })

  if (getCurrentScope())
    onScopeDispose(() => {
      disposed = true
      again = false
      asked.abort()
    })

  const which: Which[] = ['histogram', 'facets', 'rows', 'groups']
  return {
    buckets,
    bucketsAnswer,
    facets,
    rows: computed(() => pages.value.flatMap((p) => p.rows)),
    next: computed(() =>
      pagesOf.value === current.value ? (pages.value[pages.value.length - 1]?.next ?? null) : null,
    ),
    groups,
    loading,
    errors,
    error: computed(() => which.map((w) => errors.value[w]).find((e) => e != null) ?? null),
  }
}

/** Whether `after` is `before` slid by the clock: the same fields and
 * search over a window as long, later. */
function slide(before: Question, after: Question) {
  const span = (q: Question) => Date.parse(q.to) - Date.parse(q.from)
  return (
    Date.parse(after.to) > Date.parse(before.to) &&
    span(after) === span(before) &&
    JSON.stringify([after.fields, after.search]) === JSON.stringify([before.fields, before.search])
  )
}

# magic_logs_explorer

The explorer of [magic_logs](https://github.com/rsansores/magic_logs), for
Vue 3, without its looks: a window of time, facets with counts, a histogram
that zooms by brushing, the rows newest first and grouped counts. It holds
the state and works out the geometry; your components draw it.

![The demo: a day of a fleet's events, an incident in the harbor](https://github.com/rsansores/magic_logs/raw/main/explorer/images/explorer.png)

![The same, brushed to the incident and narrowed to errors, grouped by truck](https://github.com/rsansores/magic_logs/raw/main/explorer/images/explorer-zoomed.png)

The pictures are of the demo in this directory (`pnpm dev`): made-up data,
and every pixel drawn by the demo's own markup and stylesheet. The package
ships no CSS and no chart library.

```sh
pnpm add magic_logs_explorer
```

It needs Vue 3.5 or later, and is ESM only.

## How it fits

Your server runs magic_logs' four questions (`page`, `histogram`, `facets`,
`grouped`) in its own handlers, behind its own authorization, and returns
the rows. With the crate's `serde` feature they serialize to the shapes this
package types: `BucketRow`, `FacetCountRow`, `GroupRow`. The explorer asks
for them through functions you hand it, or you ask through your own data
layer.

```vue
<script setup lang="ts">
import {
  LogExplorerRoot,
  LogFacet,
  LogHistogram,
  LogRows,
  urlStore,
  useLogExplorer,
  useLogFetch,
} from 'magic_logs_explorer'

const explorer = useLogExplorer({ fields: ['level', 'kind', 'unit'], store: urlStore() })
const answers = useLogFetch<MyEvent>(explorer, {
  histogram: (q, step, signal) => get('/events/histogram', { ...q, step: step.name }, signal),
  facets: (q, names, signal) => get('/events/facets', { ...q, facets: names }, signal),
  page: (q, cursor, signal) => get('/events', { ...q, cursor }, signal),
  grouped: (q, signal) => get('/events/grouped', q, signal),
})
</script>

<template>
  <LogExplorerRoot :explorer="explorer" :answers="answers">
    <LogFacet field="level" v-slot="{ items, toggle }">
      <label v-for="i in items" :key="i.value">
        <input type="checkbox" :checked="i.chosen" @change="toggle(i.value)" />
        {{ i.value }} ({{ i.count }})
      </label>
    </LogFacet>

    <LogHistogram :levels="['info', 'warning', 'error']" v-slot="{ geometry, brushed, handlers }">
      <!-- touch-action: none, or a finger scrolls the page instead of brushing -->
      <svg viewBox="0 0 1000 100" preserveAspectRatio="none" style="touch-action: none" v-bind="handlers">
        <template v-for="bar in geometry.bars" :key="bar.at">
          <rect
            v-for="s in bar.segments"
            :key="s.level"
            :class="s.level"
            :x="bar.x * 1000"
            :width="bar.width * 1000"
            :y="100 - s.y1 * 100"
            :height="(s.y1 - s.y0) * 100"
          />
        </template>
        <rect v-if="brushed" class="brush" :x="brushed.x * 1000" :width="brushed.width * 1000" height="100" />
      </svg>
    </LogHistogram>

    <!-- :answers types the rows as yours -->
    <LogRows :answers="answers" v-slot="{ rows, more, loadMore }">
      <MyTable :rows="rows" />
      <button v-if="more" @click="loadMore">More</button>
    </LogRows>
  </LogExplorerRoot>
</template>
```

## The pieces

| Piece                                         | What it is                                                                                                                                                              |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `useLogExplorer(options)`                     | the state: `preset`, `from`, `to`, each field's chosen values, `search`, `live`, the page cursors; and from them `range`, `step` and the `question` every answer shares |
| `memoryStore`, `urlStore()`, your own `Store` | where the state lives. A router's query: `(key, initial, codec) => useRouteState(key, initial, { codec })`                                                              |
| `useLogFetch(explorer, fetchers)`             | asks the four questions with your functions, keeps each answer until the next arrives, drops an answer to a question no longer asked                                    |
| `LogExplorerRoot`                             | hands the state and the answers to the parts inside it                                                                                                                  |
| `LogHistogram` / `useHistogram`               | bars stacked by level, the scale's ticks, round instants, and pointer handlers that brush and zoom — all as fractions of the plot                                       |
| `LogFacet` / `useFacet`                       | one field's values, largest first, each counted under every other field's filter, with `toggle`, `only`, `clear`                                                        |
| `LogRows`, `LogGroups`                        | the rows read so far and whether there are more; the largest groups                                                                                                     |
| `magic_logs_explorer/core`                    | the same arithmetic with no framework: `stack`, `fill`, `geometry`, `selection`, `stepFor`, `rangeOf`, `facetOf`, the codecs                                            |

With vue-query (or anything like it), skip `useLogFetch`: key your queries
on `explorer.question`, `explorer.step` and `explorer.asked` (it moves when
the reader asks again, `refresh()`), read pages after each of
`explorer.cursors`, and either hand `LogExplorerRoot` the answers in the
`LogAnswers` shape or call `useHistogram` and `useFacet` yourself. Hand
`useHistogram` the window and step your buckets answer, too: while the next
answer is on its way, the bars stay where they were counted.

How it behaves:

- **The question changes only when its value does.** A write to the store
  that leaves it as it was — another key, a `clear()` with nothing chosen,
  a live tick on a zoomed window — asks nothing again and keeps the pages
  read.
- **Live slides a named window** every `liveEvery` ms (10 s). A slide while
  answers are on their way waits for them and asks once after, so a server
  slower than the slide still answers: a wait is given up after two
  slides, then four, then eight, until a whole round answers — and after
  one does, as many slides as that round took and one more. Anything the
  reader changes, or turning live off, is asked at once, and the answers on
  their way are dropped. Reading back (`more`),
  zooming and choosing `custom` stop live: the window holds still while a reader reads it.
- **Give your fetches a timeout.** A live wait is given up, but an answer
  that never comes holds `loading` until the next question: `fetch(url, { signal: AbortSignal.any([signal, AbortSignal.timeout(15_000)]) })`.
- **Pages follow their question.** Until a new question's first page
  arrives, the last question's rows stay on screen and `next` is null, so
  no page is read after them; `refresh()` starts the pages again too. A page
  that fails can be asked for again: its cursor is given back, and its
  error is cleared once it is answered.
- **The address is read defensively.** A preset is one of yours (not
  `toString`), an instant is RFC 3339 with its offset, a custom window
  spans at most `longest` (about two years), a field's values pass its
  `accept`; anything else reads as the default (and `shown` says so). A value may hold a comma:
  it is written `\,`.
- **Steps**: minutes for a window of up to two hours, five minutes up to
  eight hours, hours up to four days, six hours up to twenty, days past that
  — where magic_logs answers from its daily counts.
- **The window is yours to write too.** `preset`, `from` and `to` may be
  written directly, in any order and in separate ticks (a date picker): the
  store keeps what was written, and `shown` says which window is in effect —
  `custom` once the two instants read as one, the preset shown first until
  then. Draw a preset control from `shown`, not `preset`. An instant is
  written as RFC 3339 with its offset (or `''` to clear it); anything else
  throws, the same with every store. `zoom` and `choose` write all three
  for you.
- **In the browser.** `urlStore()` reads the address and `useLogFetch`
  asks as soon as it is called: on a server render, call them on the
  client (`onMounted`, or a client-only component), or the markup the
  server sends and the client's will differ. A live clock never runs
  outside a browser.
- **A bar is a whole bucket.** magic_logs' `histogram` counts every bucket
  the window touches, whole — the first and the last too, whether it reads
  the counts or the rows — so the plot spans them (`geometry.range`) and
  every bar is as wide as its step; a live window's last bar holds what has
  come so far. Label the axis and brush on `geometry.range`, not on the
  window asked. A brush or `zoomTo` ends no later than the window.
- **A stored value is written, not changed.** `urlStore()` hands its values
  frozen (the address is the value): write a new array rather than
  `push`ing into the one read.
- **One key, one thing.** A field stored under `window`, `from`, `to`, `q`
  or `live` — or two fields under one key — is refused when the explorer
  is made: give it a `key` of its own.
- **Time is UTC.** Buckets begin where TimescaleDB's `time_bucket` begins
  them, on UTC boundaries, and so do the round instants `geometry` ticks
  at: format the axis in UTC, or a day's label falls on the day before west
  of Greenwich.

## The demo

`pnpm dev` serves it on port 5198. `pnpm screenshot` writes the two pictures
above (it needs a Chromium: `CHROMIUM`, or `chromium` on the `PATH`).

## License

MIT or Apache-2.0, at your option.

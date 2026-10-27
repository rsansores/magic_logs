# magic_logs_explorer

The explorer of [magic_logs](https://github.com/rsansores/magic_logs), for
Vue 3, without its looks: a window of time, facets with counts, a histogram
that zooms by brushing, the rows newest first and grouped counts. It holds
the state and works out the geometry; your components draw it.

![The demo: a day of a fleet's events, an incident in the harbor](https://raw.githubusercontent.com/rsansores/magic_logs/main/explorer/images/explorer.png)

![The same, brushed to the incident and narrowed to errors, grouped by truck](https://raw.githubusercontent.com/rsansores/magic_logs/main/explorer/images/explorer-zoomed.png)

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
  facets: (q, fields, signal) => get('/events/facets', { ...q, fields }, signal),
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
      <svg viewBox="0 0 1000 100" preserveAspectRatio="none" v-bind="handlers">
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

    <LogRows v-slot="{ rows, more, loadMore }">
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
on `explorer.question` and `explorer.step`, read pages after each of
`explorer.cursors`, and either hand `LogExplorerRoot` the answers in the
`LogAnswers` shape or call `useHistogram` and `useFacet` yourself.

A step is minutes for a window of two hours, whole hours up to four days,
six hours up to twenty, and days past that — where magic_logs answers from
its daily counts.

## The demo

`pnpm dev` serves it on port 5198. `pnpm screenshot` writes the two pictures
above (it needs a Chromium: `CHROMIUM`, or `chromium` on the `PATH`).

## License

MIT or Apache-2.0, at your option.

<script setup lang="ts">
// The explorer over a made-up fleet. Everything a reader sees is drawn here,
// with this file's own markup and demo.css: the package hands over state,
// answers and geometry, and draws nothing.
import { computed, ref } from 'vue'
import {
  LogExplorerRoot,
  LogFacet,
  LogGroups,
  LogHistogram,
  LogRows,
  oneOf,
  urlStore,
  useLogExplorer,
  useLogFetch,
} from '../src/index.js'
import { fields, fleet, levels, server, type Event } from './server.js'

const now = Date.now()
const api = server(fleet(now))

const store = urlStore()
const explorer = useLogExplorer({ fields, store })
const answers = useLogFetch<Event>(explorer, api)
const view = store('view', 'rows', oneOf(['rows', 'grouped']))

const titles: Record<string, string> = { level: 'Level', kind: 'Kind', zone: 'Zone', unit: 'Unit' }
const presetNames: Record<string, string> = { '1h': '1 h', '24h': '24 h', '7d': '7 d', '30d': '30 d' }

const number = new Intl.NumberFormat('en')
const time = new Intl.DateTimeFormat('en', {
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  hour12: false,
})
const day = new Intl.DateTimeFormat('en', { month: 'short', day: 'numeric' })
const moment = new Intl.DateTimeFormat('en', {
  month: 'short',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  hour12: false,
})
const kindNames: Record<string, string> = {
  engine_start: 'Engine start',
  door_opened: 'Door opened',
  geofence_exit: 'Left its area',
  low_battery: 'Low battery',
  gps_lost: 'GPS lost',
  temperature_high: 'Cold chain breach',
  sync_failed: 'Upload failed',
  tamper: 'Seal broken',
}
const label = (s: string) => kindNames[s] ?? s

const total = computed(() =>
  answers.facets.value.filter((f) => f.field === 'level').reduce((s, f) => s + f.count, 0),
)
const chosen = computed(() =>
  explorer.fieldNames.flatMap((field) => explorer.fields[field].value.map((value) => ({ field, value }))),
)
const tickLabel = (at: number) => {
  const span = explorer.range.value.to.getTime() - explorer.range.value.from.getTime()
  return span > 2 * 86_400_000 ? day.format(at) : time.format(at).slice(0, 5)
}
const busiest = (rows: { count: number }[]) => Math.max(1, ...rows.map((r) => r.count))
const hovered = ref<number | null>(null)
</script>

<template>
  <LogExplorerRoot :explorer="explorer" :answers="answers">
    <div class="app">
      <header class="top">
        <div class="brand">
          <span class="mark" aria-hidden="true">◢</span>
          <div>
            <h1>Fleet events</h1>
            <p>magic_logs_explorer · made-up data, 30 days of 36 trucks</p>
          </div>
        </div>
        <div class="controls">
          <label class="search">
            <span aria-hidden="true">⌕</span>
            <input v-model="explorer.search.value" type="search" placeholder="Search messages and units" />
          </label>
          <div class="segmented" role="group" aria-label="Window">
            <button
              v-for="p in explorer.presets"
              :key="p"
              :class="{ on: explorer.preset.value === p }"
              @click="explorer.choose(p)"
            >
              {{ presetNames[p] ?? p }}
            </button>
            <button :class="{ on: explorer.preset.value === 'custom' }" disabled>Zoomed</button>
          </div>
          <button
            class="live"
            :class="{ on: explorer.live.value }"
            @click="explorer.live.value = !explorer.live.value"
          >
            <span class="dot" />Live
          </button>
        </div>
      </header>

      <aside class="facets">
        <LogFacet v-for="field in explorer.fieldNames" :key="field" :field="field" v-slot="facet">
          <section class="facet">
            <h2>
              {{ titles[field] }}
              <button v-if="facet.chosen.length" class="link" @click="facet.clear">clear</button>
            </h2>
            <ul>
              <li v-for="item in facet.items.slice(0, field === 'unit' ? 8 : 10)" :key="item.value">
                <label
                  :class="{
                    chosen: item.chosen,
                    [`lv-${item.value}`]: field === 'level',
                    mono: field === 'unit',
                  }"
                >
                  <input type="checkbox" :checked="item.chosen" @change="facet.toggle(item.value)" />
                  <span class="name" :class="{ capital: field === 'level' }">{{ label(item.value) }}</span>
                  <span class="count">{{ number.format(item.count) }}</span>
                  <span class="bar" :style="{ width: `${(item.count / busiest(facet.items)) * 100}%` }" />
                </label>
              </li>
            </ul>
          </section>
        </LogFacet>
      </aside>

      <main class="main">
        <section class="card chart">
          <div class="chart-head">
            <div>
              <strong>{{ number.format(total) }}</strong> events
              <span class="muted">
                · {{ moment.format(explorer.range.value.from) }} –
                {{ moment.format(explorer.range.value.to) }} · by
                {{ explorer.step.value.name.replace(/([A-Z])/g, ' $1').toLowerCase() }}
              </span>
            </div>
            <ul class="legend">
              <li v-for="level in levels" :key="level" :class="`lv-${level}`"><i />{{ level }}</li>
            </ul>
          </div>
          <LogHistogram :levels="levels" v-slot="{ geometry, brushed, handlers }">
            <div class="plot">
              <div class="y">
                <span
                  v-for="tick in geometry.counts"
                  :key="tick.value"
                  :style="{ bottom: `${tick.at * 100}%` }"
                >
                  {{ number.format(tick.value) }}
                </span>
              </div>
              <div class="area" v-bind="handlers" @pointerleave="hovered = null">
                <div
                  v-for="tick in geometry.counts"
                  :key="tick.value"
                  class="grid"
                  :style="{ bottom: `${tick.at * 100}%` }"
                />
                <svg viewBox="0 0 1000 100" preserveAspectRatio="none">
                  <g v-for="(bar, i) in geometry.bars" :key="bar.at" @pointerenter="hovered = i">
                    <rect
                      v-for="s in bar.segments"
                      :key="s.level"
                      :class="`lv-${s.level}`"
                      :x="bar.x * 1000 + 0.6"
                      :width="Math.max(bar.width * 1000 - 1.2, 0.6)"
                      :y="100 - s.y1 * 100"
                      :height="(s.y1 - s.y0) * 100"
                    />
                    <rect class="hit" :x="bar.x * 1000" :width="bar.width * 1000" y="0" height="100" />
                  </g>
                </svg>
                <div
                  v-if="brushed"
                  class="brush"
                  :style="{ left: `${brushed.x * 100}%`, width: `${brushed.width * 100}%` }"
                />
                <div
                  v-if="hovered != null && geometry.bars[hovered] && !brushed"
                  class="tip"
                  :style="{
                    left: `${(geometry.bars[hovered]!.x + geometry.bars[hovered]!.width / 2) * 100}%`,
                  }"
                >
                  <b>{{ moment.format(geometry.bars[hovered]!.at) }}</b>
                  <span
                    v-for="s in [...geometry.bars[hovered]!.segments].reverse()"
                    :key="s.level"
                    :class="`lv-${s.level}`"
                  >
                    <i />{{ s.level }} {{ number.format(s.count) }}
                  </span>
                </div>
              </div>
              <div class="x">
                <span v-for="tick in geometry.times" :key="tick.value" :style="{ left: `${tick.at * 100}%` }">
                  {{ tickLabel(tick.value) }}
                </span>
              </div>
            </div>
          </LogHistogram>
          <p class="hint">Drag across the bars to zoom in; click a bar for its bucket alone.</p>
        </section>

        <div class="toolbar">
          <div class="tabs" role="tablist">
            <button role="tab" :aria-selected="view === 'rows'" @click="view = 'rows'">Events</button>
            <button role="tab" :aria-selected="view === 'grouped'" @click="view = 'grouped'">Grouped</button>
          </div>
          <div class="chips">
            <button
              v-for="c in chosen"
              :key="`${c.field}:${c.value}`"
              class="chip"
              @click="explorer.toggle(c.field, c.value)"
            >
              {{ titles[c.field] }}: {{ label(c.value) }} <span aria-hidden="true">×</span>
            </button>
            <button v-if="explorer.narrowed.value" class="link" @click="explorer.clear()">Clear all</button>
          </div>
        </div>

        <section v-if="view === 'rows'" class="card table">
          <LogRows v-slot="{ rows, more, loadMore, loading }">
            <table>
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Level</th>
                  <th>Kind</th>
                  <th>Unit</th>
                  <th>Zone</th>
                  <th>Message</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="e in rows as Event[]" :key="e.id">
                  <td class="mono">
                    {{ moment.format(e.at) }}:{{ String(new Date(e.at).getSeconds()).padStart(2, '0') }}
                  </td>
                  <td>
                    <span class="pill" :class="`lv-${e.level}`">{{ e.level }}</span>
                  </td>
                  <td>{{ label(e.kind) }}</td>
                  <td class="mono">{{ e.unit }}</td>
                  <td>{{ e.zone }}</td>
                  <td class="message">{{ e.message }}</td>
                </tr>
                <tr v-if="!rows.length && !loading">
                  <td colspan="6" class="empty">No events in this window</td>
                </tr>
              </tbody>
            </table>
            <div v-if="more" class="more">
              <button :disabled="loading" @click="loadMore">
                {{ loading ? 'Loading…' : 'Load 50 more' }}
              </button>
            </div>
          </LogRows>
        </section>

        <section v-else class="card table">
          <LogGroups v-slot="{ groups }">
            <table>
              <thead>
                <tr>
                  <th>Unit</th>
                  <th>Kind</th>
                  <th class="num">Events</th>
                  <th>First</th>
                  <th>Last</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="g in groups" :key="`${g.top}:${g.value}`">
                  <td class="mono">{{ g.top }}</td>
                  <td>{{ label(g.value ?? '') }}</td>
                  <td class="num">
                    <span class="inline-bar" :style="{ width: `${(g.count / busiest(groups)) * 80}px` }" />
                    {{ number.format(g.count) }}
                  </td>
                  <td class="mono">{{ g.first_at && moment.format(new Date(g.first_at)) }}</td>
                  <td class="mono">{{ g.last_at && moment.format(new Date(g.last_at)) }}</td>
                </tr>
              </tbody>
            </table>
          </LogGroups>
        </section>
      </main>
    </div>
  </LogExplorerRoot>
</template>

// Written by magic_logs' `the_json_an_explorer_reads_is_the_contract`: UPDATE_CONTRACT=1 rewrites it.
import type { BucketRow, FacetCountRow, GroupRow, Question } from '../types.js'

export default {
  "facets": [
    {
      "count": 259,
      "field": "kind",
      "value": "door"
    },
    {
      "count": 86,
      "field": "kind",
      "value": "gps"
    },
    {
      "count": 87,
      "field": "kind",
      "value": "power"
    },
    {
      "count": 86,
      "field": "level",
      "value": "Error"
    },
    {
      "count": 174,
      "field": "level",
      "value": "Info"
    },
    {
      "count": 86,
      "field": "level",
      "value": "Warning"
    },
    {
      "count": 116,
      "field": "unit_id",
      "value": "00000000-0000-0000-0000-0000000000a1"
    },
    {
      "count": 115,
      "field": "unit_id",
      "value": "00000000-0000-0000-0000-0000000000a2"
    },
    {
      "count": 115,
      "field": "unit_id",
      "value": "00000000-0000-0000-0000-0000000000a3"
    }
  ],
  "grouped": [
    {
      "count": 87,
      "first_at": "2026-10-01T00:00:00Z",
      "last_at": "2026-10-01T23:50:00Z",
      "top": "00000000-0000-0000-0000-0000000000a1",
      "value": "door"
    },
    {
      "count": 86,
      "first_at": "2026-10-01T00:00:00Z",
      "last_at": "2026-10-01T23:40:00Z",
      "top": "00000000-0000-0000-0000-0000000000a2",
      "value": "door"
    },
    {
      "count": 86,
      "first_at": "2026-10-01T00:10:00Z",
      "last_at": "2026-10-01T23:40:00Z",
      "top": "00000000-0000-0000-0000-0000000000a3",
      "value": "door"
    },
    {
      "count": 29,
      "first_at": "2026-10-01T00:10:00Z",
      "last_at": "2026-10-01T23:30:00Z",
      "top": "00000000-0000-0000-0000-0000000000a1",
      "value": "power"
    }
  ],
  "histogram": [
    {
      "bucket": "2026-10-01T00:00:00Z",
      "count": 21,
      "level": "Error"
    },
    {
      "bucket": "2026-10-01T00:00:00Z",
      "count": 44,
      "level": "Info"
    },
    {
      "bucket": "2026-10-01T00:00:00Z",
      "count": 22,
      "level": "Warning"
    },
    {
      "bucket": "2026-10-01T06:00:00Z",
      "count": 22,
      "level": "Error"
    },
    {
      "bucket": "2026-10-01T06:00:00Z",
      "count": 43,
      "level": "Info"
    },
    {
      "bucket": "2026-10-01T06:00:00Z",
      "count": 21,
      "level": "Warning"
    },
    {
      "bucket": "2026-10-01T12:00:00Z",
      "count": 22,
      "level": "Error"
    },
    {
      "bucket": "2026-10-01T12:00:00Z",
      "count": 43,
      "level": "Info"
    },
    {
      "bucket": "2026-10-01T12:00:00Z",
      "count": 22,
      "level": "Warning"
    },
    {
      "bucket": "2026-10-01T18:00:00Z",
      "count": 21,
      "level": "Error"
    },
    {
      "bucket": "2026-10-01T18:00:00Z",
      "count": 44,
      "level": "Info"
    },
    {
      "bucket": "2026-10-01T18:00:00Z",
      "count": 21,
      "level": "Warning"
    }
  ],
  "question": {
    "fields": {
      "kind": [
        "door",
        "power"
      ]
    },
    "from": "2026-10-01T00:00:00Z",
    "to": "2026-10-02T00:00:00Z"
  },
  "step": "sixHours"
} satisfies {
  question: Question
  step: string
  histogram: BucketRow[]
  facets: FacetCountRow[]
  grouped: GroupRow[]
}

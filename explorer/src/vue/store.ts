// Where the explorer keeps its state. Each piece is a ref under a key, read
// and written through a codec, so the host decides where it lives: in
// memory, in the address bar, or in its router's query.

import { computed, getCurrentScope, onScopeDispose, ref, shallowRef, type Ref } from 'vue'
import type { Codec } from '../core/codec.js'

/** A ref for `key`, `initial` until something is set, read and written
 * through `codec`. A host with a router hands its own:
 * `(key, initial, codec) => useRouteState(key, initial, { codec })`. */
export type Store = <T>(key: string, initial: T, codec: Codec<T>) => Ref<T>

/** Plain refs: the state lives as long as the explorer does. */
export const memoryStore: Store = <T>(_key: string, initial: T) => ref(initial) as Ref<T>

/** The address bar's query, written with `history.replaceState` so the back
 * button leaves the page rather than stepping through every filter. A value
 * equal to its initial one is left out of the address. It reads the address
 * when made and on the back button: an app whose router changes the query
 * hands the router's own store instead. Outside a browser, plain refs. */
export function urlStore(): Store {
  if (typeof window === 'undefined') return memoryStore
  const params = shallowRef(new URLSearchParams(window.location.search))
  const reread = () => (params.value = new URLSearchParams(window.location.search))
  window.addEventListener('popstate', reread)
  if (getCurrentScope()) onScopeDispose(() => window.removeEventListener('popstate', reread))

  return <T>(key: string, initial: T, codec: Codec<T>): Ref<T> => {
    // This key's own text: a write to another key leaves it, and whatever
    // reads it, as they were.
    const raw = computed(() => params.value.get(key))
    return computed<T>({
      // Frozen: the address is the value, so a host that changes what it
      // read (`push`) is told at once — it writes a new value instead.
      get: () =>
        frozen(
          raw.value == null ? structuredClone(initial) : (codec.parse(raw.value) ?? structuredClone(initial)),
        ),
      set(value) {
        const text = codec.serialize(value)
        const put = (into: URLSearchParams) => {
          if (text === codec.serialize(initial)) into.delete(key)
          else into.set(key, text)
          return into
        }
        // The address as it is, this key changed; and what this store read
        // of it, this key changed too — another key moves on the back
        // button, not on an unrelated write.
        const query = put(new URLSearchParams(window.location.search)).toString()
        window.history.replaceState(
          window.history.state,
          '',
          `${window.location.pathname}${query ? `?${query}` : ''}${window.location.hash}`,
        )
        params.value = put(new URLSearchParams(params.value))
      },
    })
  }
}

function frozen<T>(value: T): T {
  return typeof value === 'object' && value !== null ? Object.freeze(value) : value
}

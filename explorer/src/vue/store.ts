// Where the explorer keeps its state. Each piece is a ref under a key, read
// and written through a codec, so the host decides where it lives: in
// memory, in the address bar, or in its router's query.

import { customRef, getCurrentScope, onScopeDispose, ref, shallowRef, type Ref } from 'vue'
import type { Codec } from '../core/codec.js'

/** A ref for `key`, `initial` until something is set, read and written
 * through `codec`. A host with a router hands its own:
 * `(key, initial, codec) => useRouteState(key, initial, { codec })`. */
export type Store = <T>(key: string, initial: T, codec: Codec<T>) => Ref<T>

/** Plain refs: the state lives as long as the explorer does. */
export const memoryStore: Store = <T>(_key: string, initial: T) => ref(initial) as Ref<T>

/** The address bar's query, written with `history.replaceState` so the back
 * button leaves the page rather than stepping through every filter. A value
 * equal to its initial one is left out of the address. Outside a browser,
 * plain refs. */
export function urlStore(): Store {
  if (typeof window === 'undefined') return memoryStore
  const params = shallowRef(new URLSearchParams(window.location.search))
  const reread = () => (params.value = new URLSearchParams(window.location.search))
  window.addEventListener('popstate', reread)
  if (getCurrentScope()) onScopeDispose(() => window.removeEventListener('popstate', reread))

  return <T>(key: string, initial: T, codec: Codec<T>) =>
    customRef<T>((track, trigger) => ({
      get() {
        track()
        const raw = params.value.get(key)
        return raw == null ? initial : (codec.parse(raw) ?? initial)
      },
      set(value) {
        const next = new URLSearchParams(window.location.search)
        const raw = codec.serialize(value)
        if (raw === codec.serialize(initial)) next.delete(key)
        else next.set(key, raw)
        const query = next.toString()
        window.history.replaceState(
          window.history.state,
          '',
          `${window.location.pathname}${query ? `?${query}` : ''}${window.location.hash}`,
        )
        params.value = next
        trigger()
      },
    }))
}

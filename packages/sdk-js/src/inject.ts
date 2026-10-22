import { createSdk, type Runtime } from './sdk.js'
import type { BridgeEvent, BridgeRequest } from './types.js'

interface TauriInternals {
  invoke(command: string, args: unknown): Promise<unknown>
}

interface Bootstrap {
  id: string
  version: string
  path: string
}

/** The page globals the native shell prepares before this script runs. */
interface ShellWindow {
  __TAURI_INTERNALS__?: TauriInternals
  __ILMEK_BOOTSTRAP__?: Bootstrap
  __ilmekDeliver?: (json: string) => void
  ilmek?: unknown
}

/** Installs `window.ilmek` once; later attempts by page scripts cannot replace it. */
export function install(target: ShellWindow): void {
  const internals = target.__TAURI_INTERNALS__
  const bootstrap = target.__ILMEK_BOOTSTRAP__
  if (internals === undefined || bootstrap === undefined) return
  delete target.__ILMEK_BOOTSTRAP__

  let sink: (event: BridgeEvent) => void = () => undefined
  const runtime: Runtime = {
    transport: {
      request: (message: BridgeRequest) => internals.invoke('ilmek_bridge', { request: message }),
    },
    setEventSink: (next) => {
      sink = next
    },
  }
  const sdk = createSdk(runtime, bootstrap)

  Object.defineProperty(target, '__ilmekDeliver', {
    value: (json: string) => {
      sink(JSON.parse(json) as BridgeEvent)
    },
    writable: false,
    configurable: false,
  })
  Object.defineProperty(target, 'ilmek', {
    value: Object.freeze(sdk),
    writable: false,
    configurable: false,
  })
}

install(globalThis as unknown as ShellWindow)

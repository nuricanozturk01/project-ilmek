import { createSdk, type Runtime } from './sdk.js'
import type { BridgeEvent, BridgeRequest } from './types.js'

interface Bootstrap {
  id: string
  version: string
  path: string
}

interface MessageLike {
  source: unknown
  data: unknown
}

/** What this script needs of the page it runs in. The page is always a sandboxed frame of the shell. */
interface ShellWindow {
  __ILMEK_BOOTSTRAP__?: Bootstrap
  parent?: { postMessage(message: unknown, targetOrigin: string): void }
  addEventListener(type: 'message', listener: (event: MessageLike) => void): void
  ilmek?: unknown
}

interface Pending {
  resolve(response: unknown): void
}

function isMessage(data: unknown): data is Record<string, unknown> {
  return typeof data === 'object' && data !== null && (data as { ilmek?: unknown }).ilmek === 1
}

/**
 * Installs `window.ilmek` once; later attempts by page scripts cannot replace it.
 *
 * The page cannot reach the network or any native command. Its only channel is
 * `postMessage` to the parent (the shell), and it believes only messages whose source is that
 * parent. The shell decides which application a request belongs to by which frame sent it.
 */
export function install(target: ShellWindow & object): void {
  const bootstrap = target.__ILMEK_BOOTSTRAP__
  const shell = target.parent
  if (bootstrap === undefined || shell === undefined || shell === (target as unknown)) return
  delete target.__ILMEK_BOOTSTRAP__

  const pending = new Map<string, Pending>()
  let counter = 0
  let sink: (event: BridgeEvent) => void = () => undefined

  target.addEventListener('message', (event) => {
    if (event.source !== shell || !isMessage(event.data)) return
    const message = event.data
    if (typeof message.event === 'string') {
      sink(JSON.parse(message.event) as BridgeEvent)
      return
    }
    if (typeof message.id !== 'string') return
    const waiting = pending.get(message.id)
    if (waiting === undefined) return
    pending.delete(message.id)
    waiting.resolve(message.response)
  })

  const runtime: Runtime = {
    transport: {
      request: (request: BridgeRequest) =>
        new Promise<unknown>((resolve) => {
          counter += 1
          const id = `r${String(counter)}`
          pending.set(id, { resolve })
          shell.postMessage({ ilmek: 1, id, request }, '*')
        }),
    },
    setEventSink: (next) => {
      sink = next
    },
  }

  Object.defineProperty(target, 'ilmek', {
    value: Object.freeze(createSdk(runtime, bootstrap)),
    writable: false,
    configurable: false,
  })
}

install(globalThis)

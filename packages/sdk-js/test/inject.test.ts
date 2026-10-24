import { describe, expect, it, vi } from 'vitest'

type Listener = (event: { source: unknown; data: unknown }) => void

interface FakeWindow {
  __ILMEK_BOOTSTRAP__?: { id: string; version: string; path: string }
  parent?: { postMessage: (message: unknown, targetOrigin: string) => void }
  addEventListener: (type: 'message', listener: Listener) => void
  ilmek?: unknown
}

const bootstrap = { id: '01a1647f-de10-755f-beb4-79875d9e6368', version: '1.0.0', path: 'x' }

/** A page inside a frame: `shell` is its parent, and `deliver` plays a message arriving. */
function page() {
  const shell = { postMessage: vi.fn<(message: unknown, origin: string) => void>() }
  const listeners: Listener[] = []
  const target: FakeWindow = {
    __ILMEK_BOOTSTRAP__: { ...bootstrap },
    parent: shell,
    addEventListener: (_type, listener) => {
      listeners.push(listener)
    },
  }
  const deliver = (source: unknown, data: unknown) => {
    for (const listener of listeners) listener({ source, data })
  }
  return { target, shell, deliver }
}

async function loadInto(target: FakeWindow): Promise<void> {
  vi.resetModules()
  const { install } = await import('../src/inject.js')
  install(target)
}

function lastRequest(shell: { postMessage: { mock: { calls: unknown[][] } } }) {
  const call = shell.postMessage.mock.calls.at(-1)
  return call?.[0] as { ilmek: number; id: string; request: unknown }
}

describe('install', () => {
  it('exposes a frozen, non-replaceable window.ilmek and hides the bootstrap data', async () => {
    const { target, shell, deliver } = page()
    await loadInto(target)

    expect(target.__ILMEK_BOOTSTRAP__).toBeUndefined()
    const sdk = target.ilmek as { app: typeof bootstrap; connection: { state(): Promise<string> } }
    expect(sdk.app).toEqual(bootstrap)
    expect(Object.isFrozen(sdk)).toBe(true)
    expect(() => {
      'use strict'
      target.ilmek = {}
    }).toThrow(TypeError)
    expect(() => Object.defineProperty(target, 'ilmek', { value: 1 })).toThrow(TypeError)

    const state = sdk.connection.state()
    const sent = lastRequest(shell)
    expect(sent).toMatchObject({ ilmek: 1, request: { v: 1, op: 'connection.state' } })
    expect(shell.postMessage).toHaveBeenCalledWith(expect.anything(), '*')
    deliver(shell, { ilmek: 1, id: sent.id, response: { ok: true, result: 'connected' } })
    await expect(state).resolves.toBe('connected')
  })

  it('believes only its parent: forged replies and events from anywhere else are ignored', async () => {
    const { target, shell, deliver } = page()
    await loadInto(target)
    const sdk = target.ilmek as { connection: { state(): Promise<string> } }

    let settled = false
    const state = sdk.connection.state().then((value) => {
      settled = true
      return value
    })
    const sent = lastRequest(shell)
    deliver({ some: 'other window' }, { ilmek: 1, id: sent.id, response: { ok: true, result: 'x' } })
    deliver(target, { ilmek: 1, id: sent.id, response: { ok: true, result: 'self' } })
    await Promise.resolve()
    expect(settled).toBe(false)

    deliver(shell, { ilmek: 1, id: sent.id, response: { ok: true, result: 'connected' } })
    await expect(state).resolves.toBe('connected')
  })

  it('delivers pushed events as JSON text, never as code', async () => {
    const { target, shell, deliver } = page()
    await loadInto(target)
    const sdk = target.ilmek as {
      subscribe(t: string, c: null, h: (e: unknown) => void): Promise<unknown>
    }
    const handler = vi.fn()
    const subscribed = sdk.subscribe('a.b', null, handler)
    const sent = lastRequest(shell)
    deliver(shell, { ilmek: 1, id: sent.id, response: { ok: true, result: { subscriptionId: 's' } } })
    await subscribed

    deliver(shell, {
      ilmek: 1,
      event: JSON.stringify({ kind: 'event', subscriptionId: 's', event: { text: '</script><b>' } }),
    })
    expect(handler).toHaveBeenCalledWith({ text: '</script><b>' })
  })

  it('does nothing outside a frame of the shell', async () => {
    const alone: FakeWindow = { __ILMEK_BOOTSTRAP__: { ...bootstrap }, addEventListener: vi.fn() }
    await loadInto(alone)
    expect(alone.ilmek).toBeUndefined()

    const toplevel = page().target
    toplevel.parent = toplevel as unknown as NonNullable<FakeWindow['parent']>
    await loadInto(toplevel)
    expect(toplevel.ilmek).toBeUndefined()

    const bare: FakeWindow = { addEventListener: vi.fn(), parent: { postMessage: vi.fn() } }
    await loadInto(bare)
    expect(bare.ilmek).toBeUndefined()
  })
})

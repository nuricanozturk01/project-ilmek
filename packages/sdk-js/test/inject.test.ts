import { describe, expect, it, vi } from 'vitest'

interface FakeWindow {
  __TAURI_INTERNALS__?: { invoke: (command: string, args: unknown) => Promise<unknown> }
  __ILMEK_BOOTSTRAP__?: { id: string; version: string; path: string }
  __ilmekDeliver?: (json: string) => void
  ilmek?: unknown
}

async function loadInto(target: FakeWindow): Promise<void> {
  vi.resetModules()
  const { install } = await import('../src/inject.js')
  install(target)
}

const bootstrap = { id: '01a1647f-de10-755f-beb4-79875d9e6368', version: '1.0.0', path: 'x' }

describe('install', () => {
  it('exposes a frozen, non-replaceable window.ilmek and hides the bootstrap data', async () => {
    const invoke = vi.fn(() => Promise.resolve({ ok: true, result: 'connected' }))
    const target: FakeWindow = { __TAURI_INTERNALS__: { invoke }, __ILMEK_BOOTSTRAP__: bootstrap }
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

    await expect(sdk.connection.state()).resolves.toBe('connected')
    expect(invoke).toHaveBeenCalledWith('ilmek_bridge', {
      request: { v: 1, op: 'connection.state' },
    })
  })

  it('delivers pushed events as JSON text, never as code', async () => {
    const target: FakeWindow = {
      __TAURI_INTERNALS__: {
        invoke: () => Promise.resolve({ ok: true, result: { subscriptionId: 's' } }),
      },
      __ILMEK_BOOTSTRAP__: bootstrap,
    }
    await loadInto(target)
    const sdk = target.ilmek as {
      subscribe(t: string, c: null, h: (e: unknown) => void): Promise<unknown>
    }
    const handler = vi.fn()
    await sdk.subscribe('a.b', null, handler)
    target.__ilmekDeliver?.(
      JSON.stringify({ kind: 'event', subscriptionId: 's', event: { text: '</script><b>' } }),
    )
    expect(handler).toHaveBeenCalledWith({ text: '</script><b>' })
    expect(() =>
      Object.defineProperty(target, '__ilmekDeliver', { value: () => undefined }),
    ).toThrow(TypeError)
  })

  it('does nothing outside a shell-prepared page', async () => {
    const target: FakeWindow = {}
    await loadInto(target)
    expect(target.ilmek).toBeUndefined()
  })
})

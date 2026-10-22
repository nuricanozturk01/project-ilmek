import { describe, expect, it, vi } from 'vitest'
import { createSdk, type Runtime } from '../src/sdk.js'
import { IlmekError, type BridgeEvent, type BridgeRequest } from '../src/types.js'
import { isUuidv7 } from '../src/uuid7.js'

const APP = { id: '01a1647f-de10-755f-beb4-79875d9e6368', version: '1.0.0', path: '' }

function harness(
  respond: (request: BridgeRequest) => unknown = () => ({ ok: true, result: null }),
) {
  const sent: BridgeRequest[] = []
  let push: (event: BridgeEvent) => void = () => undefined
  const runtime: Runtime = {
    transport: {
      request: (message) => {
        sent.push(message)
        return Promise.resolve(respond(message))
      },
    },
    setEventSink: (sink) => {
      push = sink
    },
  }
  return {
    sdk: createSdk(runtime, APP),
    sent,
    push: (event: BridgeEvent) => {
      push(event)
    },
  }
}

describe('call', () => {
  it('sends reads without an operation id', async () => {
    const { sdk, sent } = harness(() => ({ ok: true, result: { topics: [] } }))
    await expect(sdk.call('forum.topics.list', { limit: 5 })).resolves.toEqual({ topics: [] })
    expect(sent).toEqual([
      {
        v: 1,
        op: 'call',
        method: 'forum.topics.list',
        input: { limit: 5 },
        write: false,
        operationId: null,
        timeoutMs: null,
      },
    ])
  })

  it('gives every write its own fresh UUIDv7 operation id', async () => {
    const { sdk, sent } = harness()
    await sdk.call('forum.topics.create', { title: 'a' }, { write: true })
    await sdk.call('forum.topics.create', { title: 'a' }, { write: true })
    const ids = sent.map((request) => (request.op === 'call' ? request.operationId : null))
    expect(ids.every((id) => id !== null && isUuidv7(id))).toBe(true)
    expect(ids[0]).not.toBe(ids[1])
  })

  it('keeps a caller-supplied operation id, treating the call as a write', async () => {
    const { sdk, sent } = harness()
    const operationId = '01a1647f-de10-755f-beb4-79875d9e6368'
    await sdk.call('notes.update', { id: 1 }, { operationId, timeoutMs: 4000 })
    expect(sent[0]).toMatchObject({ write: true, operationId, timeoutMs: 4000 })
  })

  it('rejects malformed calls before anything reaches the runtime', async () => {
    const { sdk, sent } = harness()
    await expect(sdk.call('Bad Method', {})).rejects.toMatchObject({ code: 'INVALID_MESSAGE' })
    await expect(sdk.call('ok.method', [])).rejects.toMatchObject({ code: 'INVALID_MESSAGE' })
    await expect(sdk.call('ok.method', null)).rejects.toMatchObject({ code: 'INVALID_MESSAGE' })
    await expect(
      sdk.call('ok.method', {}, { operationId: 'f47ac10b-58cc-4372-a567-0e02b2c3d479' }),
    ).rejects.toMatchObject({
      code: 'INVALID_MESSAGE',
    })
    expect(sent).toHaveLength(0)
  })

  it('surfaces server errors with their retry semantics', async () => {
    const { sdk } = harness(() => ({
      ok: false,
      error: { code: 'TIMEOUT', message: 'no answer', retryable: true, outcomeUnknown: true },
    }))
    const error = await sdk.call('a.b', {}, { write: true }).catch((e: unknown) => e)
    expect(error).toBeInstanceOf(IlmekError)
    expect(error).toMatchObject({ code: 'TIMEOUT', retryable: true, outcomeUnknown: true })
  })

  it('treats malformed runtime answers as internal errors', async () => {
    const { sdk } = harness(() => 'garbage')
    await expect(sdk.call('a.b', {})).rejects.toMatchObject({ code: 'INTERNAL' })
    const broken = harness(() => ({ ok: false, error: 7 }))
    await expect(broken.sdk.call('a.b', {})).rejects.toMatchObject({ code: 'INTERNAL' })
  })
})

describe('subscribe', () => {
  it('routes events to the matching subscription only and stops after unsubscribe', async () => {
    const { sdk, sent, push } = harness((request) =>
      request.op === 'subscribe'
        ? { ok: true, result: { subscriptionId: request.topic } }
        : { ok: true, result: null },
    )
    const first = vi.fn()
    const second = vi.fn()
    const stopFirst = await sdk.subscribe('forum.events', null, first)
    await sdk.subscribe('other.events', null, second)

    push({ kind: 'event', subscriptionId: 'forum.events', event: { n: 1 } })
    expect(first).toHaveBeenCalledWith({ n: 1 })
    expect(second).not.toHaveBeenCalled()

    stopFirst()
    push({ kind: 'event', subscriptionId: 'forum.events', event: { n: 2 } })
    expect(first).toHaveBeenCalledTimes(1)
    expect(sent.at(-1)).toEqual({ v: 1, op: 'unsubscribe', subscriptionId: 'forum.events' })
  })

  it('forgets a subscription the runtime ended', async () => {
    const { sdk, push } = harness(() => ({ ok: true, result: { subscriptionId: 's1' } }))
    const handler = vi.fn()
    await sdk.subscribe('a.b', 'cursor', handler)
    push({ kind: 'subscription_ended', subscriptionId: 's1', error: null })
    push({ kind: 'event', subscriptionId: 's1', event: 1 })
    expect(handler).not.toHaveBeenCalled()
  })

  it('validates the topic and the subscription answer', async () => {
    const { sdk } = harness(() => ({ ok: true, result: {} }))
    await expect(sdk.subscribe('NOPE', null, () => undefined)).rejects.toMatchObject({
      code: 'INVALID_MESSAGE',
    })
    await expect(sdk.subscribe('a.b', null, () => undefined)).rejects.toMatchObject({
      code: 'INTERNAL',
    })
  })
})

describe('connection and storage', () => {
  it('reports the connection state and notifies listeners until they unsubscribe', async () => {
    const { sdk, push } = harness(() => ({ ok: true, result: 'connected' }))
    await expect(sdk.connection.state()).resolves.toBe('connected')
    const seen: string[] = []
    const stop = sdk.connection.onChange((state) => seen.push(state))
    push({ kind: 'connection', state: 'disconnected' })
    stop()
    push({ kind: 'connection', state: 'connected' })
    expect(seen).toEqual(['disconnected'])
  })

  it('maps unknown connection answers to disconnected', async () => {
    const { sdk } = harness(() => ({ ok: true, result: 'weird' }))
    await expect(sdk.connection.state()).resolves.toBe('disconnected')
  })

  it('passes storage operations through', async () => {
    const { sdk, sent } = harness(() => ({ ok: true, result: { a: 1 } }))
    await expect(sdk.storage.get('draft')).resolves.toEqual({ a: 1 })
    await sdk.storage.set('draft', { a: 2 })
    await sdk.storage.remove('draft')
    expect(sent.map((r) => r.op)).toEqual(['storage.get', 'storage.set', 'storage.remove'])
  })

  it('exposes the application identity it was created with', () => {
    expect(harness().sdk.app).toEqual(APP)
  })
})

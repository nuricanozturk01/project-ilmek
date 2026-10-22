import type {
  BridgeEvent,
  BridgeFailure,
  BridgeRequest,
  CallOptions,
  ConnectionState,
  IlmekAppSDK,
  Transport,
} from './types.js'
import { IlmekError } from './types.js'
import { isUuidv7, uuidv7 } from './uuid7.js'

const METHOD = /^[a-z][a-z0-9_.]{0,63}$/

function fail(code: string, message: string): IlmekError {
  return new IlmekError({ code, message, retryable: false, outcomeUnknown: false })
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function readFailure(value: unknown): BridgeFailure {
  if (isRecord(value) && typeof value.code === 'string' && typeof value.message === 'string') {
    return {
      code: value.code,
      message: value.message,
      retryable: value.retryable === true,
      outcomeUnknown: value.outcomeUnknown === true,
      ...(typeof value.operationId === 'string' ? { operationId: value.operationId } : {}),
    }
  }
  return {
    code: 'INTERNAL',
    message: 'malformed error from the runtime',
    retryable: false,
    outcomeUnknown: false,
  }
}

export interface AppInfo {
  id: string
  version: string
  path: string
}

/** What the page-facing SDK needs from the host environment. */
export interface Runtime {
  transport: Transport
  /** Registers the single sink for events pushed by the native runtime. */
  setEventSink(sink: (event: BridgeEvent) => void): void
  newOperationId?: () => string
}

export function createSdk(runtime: Runtime, app: AppInfo): IlmekAppSDK {
  const subscriptions = new Map<string, (event: unknown) => void>()
  const connectionHandlers = new Set<(state: ConnectionState) => void>()
  const newOperationId = runtime.newOperationId ?? uuidv7

  async function request(message: BridgeRequest): Promise<unknown> {
    const response = await runtime.transport.request(message)
    if (!isRecord(response) || typeof response.ok !== 'boolean') {
      throw fail('INTERNAL', 'malformed response from the runtime')
    }
    if (!response.ok) throw new IlmekError(readFailure(response.error))
    return response.result
  }

  runtime.setEventSink((event) => {
    if (event.kind === 'connection') {
      for (const handler of connectionHandlers) handler(event.state)
    } else if (event.kind === 'event') {
      subscriptions.get(event.subscriptionId)?.(event.event)
    } else {
      subscriptions.delete(event.subscriptionId)
    }
  })

  async function call<T>(method: string, input: unknown, options: CallOptions = {}): Promise<T> {
    if (!METHOD.test(method)) throw fail('INVALID_MESSAGE', 'method name is not valid')
    if (!isRecord(input)) throw fail('INVALID_MESSAGE', 'input must be a JSON object')
    const write = options.write ?? options.operationId !== undefined
    if (options.operationId !== undefined && !isUuidv7(options.operationId)) {
      throw fail('INVALID_MESSAGE', 'operationId must be a UUIDv7')
    }
    const operationId = write ? (options.operationId ?? newOperationId()) : null
    const result = await request({
      v: 1,
      op: 'call',
      method,
      input,
      write,
      operationId,
      timeoutMs: options.timeoutMs ?? null,
    })
    return result as T
  }

  return {
    app: { ...app },
    connection: {
      async state() {
        const state = await request({ v: 1, op: 'connection.state' })
        return state === 'connected' ? 'connected' : 'disconnected'
      },
      onChange(handler) {
        connectionHandlers.add(handler)
        return () => connectionHandlers.delete(handler)
      },
    },
    call,
    async subscribe(topic, cursor, handler) {
      if (!METHOD.test(topic)) throw fail('INVALID_MESSAGE', 'topic name is not valid')
      const result = await request({ v: 1, op: 'subscribe', topic, cursor })
      const id = isRecord(result) ? result.subscriptionId : undefined
      if (typeof id !== 'string') throw fail('INTERNAL', 'malformed subscription from the runtime')
      subscriptions.set(id, handler)
      return () => {
        subscriptions.delete(id)
        void request({ v: 1, op: 'unsubscribe', subscriptionId: id }).catch(() => undefined)
      }
    },
    storage: {
      get: (key) => request({ v: 1, op: 'storage.get', key }),
      async set(key, value) {
        await request({ v: 1, op: 'storage.set', key, value })
      },
      async remove(key) {
        await request({ v: 1, op: 'storage.remove', key })
      },
    },
  }
}

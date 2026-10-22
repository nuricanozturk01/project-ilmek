export type ConnectionState = 'connected' | 'disconnected'

export interface CallOptions {
  /** Makes the call a write with this id (a UUIDv7); repeated calls with it apply once. */
  operationId?: string
  timeoutMs?: number
  /** The method changes state. An operation id is generated when none is given. */
  write?: boolean
}

export interface IlmekAppSDK {
  app: { id: string; version: string; path: string }
  connection: {
    state(): Promise<ConnectionState>
    onChange(handler: (state: ConnectionState) => void): () => void
  }
  call<T = unknown>(method: string, input: unknown, options?: CallOptions): Promise<T>
  subscribe(
    topic: string,
    cursor: string | null,
    handler: (event: unknown) => void,
  ): Promise<() => void>
  storage: {
    get(key: string): Promise<unknown>
    set(key: string, value: unknown): Promise<void>
    remove(key: string): Promise<void>
  }
}

/** Requests from the page to the native runtime. The runtime never trusts an app id from here. */
export type BridgeRequest =
  | {
      v: 1
      op: 'call'
      method: string
      input: unknown
      write: boolean
      operationId: string | null
      timeoutMs: number | null
    }
  | { v: 1; op: 'subscribe'; topic: string; cursor: string | null }
  | { v: 1; op: 'unsubscribe'; subscriptionId: string }
  | { v: 1; op: 'storage.get'; key: string }
  | { v: 1; op: 'storage.set'; key: string; value: unknown }
  | { v: 1; op: 'storage.remove'; key: string }
  | { v: 1; op: 'connection.state' }

export interface BridgeFailure {
  code: string
  message: string
  retryable: boolean
  outcomeUnknown: boolean
  /** For writes: repeat the call with this id to learn the outcome safely. */
  operationId?: string
}

export type BridgeResponse = { ok: true; result: unknown } | { ok: false; error: BridgeFailure }

/** Pushed from the native runtime to the page. */
export type BridgeEvent =
  | { kind: 'connection'; state: ConnectionState }
  | { kind: 'event'; subscriptionId: string; event: unknown }
  | { kind: 'subscription_ended'; subscriptionId: string; error: BridgeFailure | null }

export interface Transport {
  request(message: BridgeRequest): Promise<unknown>
}

export class IlmekError extends Error {
  readonly code: string
  readonly retryable: boolean
  /** The request may or may not have been applied; retry with the same operation id. */
  readonly outcomeUnknown: boolean
  readonly operationId: string | undefined

  constructor(failure: BridgeFailure) {
    super(failure.message)
    this.name = 'IlmekError'
    this.code = failure.code
    this.retryable = failure.retryable
    this.outcomeUnknown = failure.outcomeUnknown
    this.operationId = failure.operationId
  }
}

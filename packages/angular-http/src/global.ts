export interface IlmekCallOptions {
  operationId?: string
  timeoutMs?: number
  write?: boolean
}

export interface IlmekCaller {
  call<T = unknown>(method: string, input: unknown, options?: IlmekCallOptions): Promise<T>
}

declare global {
  interface Window {
    ilmek?: IlmekCaller
  }
}

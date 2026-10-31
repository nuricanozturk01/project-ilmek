// Type-only copy of the window.ilmek contract from packages/sdk-js/src/types.ts (lines 1-28).
// The shell injects the runtime object; this file must not import or bundle the SDK.
export type ConnectionState = 'connected' | 'disconnected';

export interface CallOptions {
  operationId?: string;
  timeoutMs?: number;
  write?: boolean;
}

export interface IlmekAppSDK {
  app: { id: string; version: string; path: string };
  connection: {
    state(): Promise<ConnectionState>;
    onChange(handler: (state: ConnectionState) => void): () => void;
  };
  call<T = unknown>(method: string, input: unknown, options?: CallOptions): Promise<T>;
  subscribe(
    topic: string,
    cursor: string | null,
    handler: (event: unknown) => void,
  ): Promise<() => void>;
  storage: {
    get(key: string): Promise<unknown>;
    set(key: string, value: unknown): Promise<void>;
    remove(key: string): Promise<void>;
  };
}

declare global {
  interface Window {
    ilmek: IlmekAppSDK;
  }
}

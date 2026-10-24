import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/** A failure from the native shell: a stable code the UI translates. */
export class ShellError extends Error {
  readonly code: string

  constructor(code: string) {
    super(code)
    this.name = 'ShellError'
    this.code = code
  }
}

export interface Status {
  community_id: string
  peer_id: string
  registry: string
  registry_reachable: boolean
}

export interface ShellState {
  joined: boolean
  status: Status | null
}

export interface CatalogEntry {
  address: string
  name: string
  description: string
  publisher_id: string
  version: string
  availability: 'recent_heartbeat' | 'stale' | 'unknown'
  cached: boolean
}

export interface CatalogPage {
  entries: CatalogEntry[]
  next_cursor: string | null
}

export interface CachedApp {
  address: string
  version: string
  bytes: number
}

export interface History {
  visited: string[]
  bookmarks: string[]
}

export interface Opened {
  label: string
  stale: boolean
}

export interface TransferProgress {
  id: string
  bytes_done: number
  bytes_total: number
}

function isCode(value: unknown): value is { code: string } {
  return (
    typeof value === 'object' && value !== null && 'code' in value && typeof value.code === 'string'
  )
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    throw new ShellError(isCode(error) ? error.code : 'unknown')
  }
}

export const shell = {
  state: () => call<ShellState>('shell_state'),
  join: (profile: string | null, registry: string | null) =>
    call<null>('shell_join', { profile, registry }),
  refresh: () => call<Status>('shell_refresh'),
  catalog: (query: string, cursor: string | null) =>
    call<CatalogPage>('shell_catalog', { query, cursor }),
  open: (address: string, transferId: string) =>
    call<Opened>('shell_open', { address, transferId }),
  cancel: (transferId: string) => call<null>('shell_cancel', { transferId }),
  cached: () => call<CachedApp[]>('shell_cached'),
  remove: (address: string) => call<null>('shell_remove', { address }),
  history: () => call<History>('shell_history'),
  bookmark: (address: string, on: boolean) => call<null>('shell_bookmark', { address, on }),
  diagnostics: () => call<string>('shell_diagnostics'),
}

export function onTransferProgress(
  handler: (progress: TransferProgress) => void,
): Promise<() => void> {
  return listen<TransferProgress>('transfer-progress', (event) => {
    handler(event.payload)
  })
}

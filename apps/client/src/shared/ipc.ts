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
  /** Names the application's session in the shell; the frame's messages are routed by it. */
  label: string
  title: string
  stale: boolean
  /** What the application's frame loads. */
  url: string
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
  /** Records the application tab `label` as the one on screen, or (`null`) a shell page. */
  showTab: (label: string | null) => call<null>('shell_show_tab', { label }),
  closeTab: (label: string) => call<null>('shell_close_tab', { label }),
  /** Relays an application page's request to that tab's session. */
  bridge: (label: string, request: unknown) => call<unknown>('shell_bridge', { label, request }),
  /** `ilmek://` addresses the program was started or handed since the last call. */
  pending: () => call<string[]>('shell_pending'),
}

export function onTabDiscarded(handler: (label: string) => void): Promise<() => void> {
  return listen<string>('tab-discarded', (event) => {
    handler(event.payload)
  })
}

export function onOpenPending(handler: () => void): Promise<() => void> {
  return listen<null>('open-pending', () => {
    handler()
  })
}

export function onTransferProgress(
  handler: (progress: TransferProgress) => void,
): Promise<() => void> {
  return listen<TransferProgress>('transfer-progress', (event) => {
    handler(event.payload)
  })
}

export interface AppEvent {
  label: string
  json: string
}

export function onAppEvent(handler: (event: AppEvent) => void): Promise<() => void> {
  return listen<AppEvent>('app-event', (event) => {
    handler(event.payload)
  })
}

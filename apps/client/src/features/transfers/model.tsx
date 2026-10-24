import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  type ReactNode,
} from 'react'
import { ShellError, onTransferProgress, shell, type Opened } from '../../shared/ipc.js'

export interface Transfer {
  id: string
  address: string
  state: 'connecting' | 'downloading' | 'done' | 'failed'
  done: number
  total: number
  stale: boolean
  error: string | null
}

type Action =
  | { type: 'add'; transfer: Transfer }
  | { type: 'progress'; id: string; done: number; total: number }
  | { type: 'finish'; id: string; stale: boolean }
  | { type: 'fail'; id: string; error: string }
  | { type: 'dismiss'; id: string }
  | { type: 'clear' }

export function isActive(transfer: Transfer): boolean {
  return transfer.state === 'connecting' || transfer.state === 'downloading'
}

function reduce(state: Transfer[], action: Action): Transfer[] {
  switch (action.type) {
    case 'add':
      return [action.transfer, ...state]
    case 'progress':
      return state.map((t) =>
        t.id === action.id
          ? { ...t, state: 'downloading', done: action.done, total: action.total }
          : t,
      )
    case 'finish':
      return state.map((t) =>
        t.id === action.id ? { ...t, state: 'done', stale: action.stale } : t,
      )
    case 'fail':
      return state.map((t) =>
        t.id === action.id ? { ...t, state: 'failed', error: action.error } : t,
      )
    case 'dismiss':
      return state.filter((t) => t.id !== action.id)
    case 'clear':
      return state.filter(isActive)
  }
}

interface TransferApi {
  transfers: Transfer[]
  /** Fetches and opens `address`; resolves to the opened tab, or `null` when it failed. */
  start: (address: string, id?: string) => Promise<Opened | null>
  cancel: (id: string) => void
  /** Starts the same address again and removes the failed attempt. */
  retry: (id: string) => void
  dismiss: (id: string) => void
  /** Removes every finished or failed entry. */
  clearFinished: () => void
  /** `true` while this address is being fetched or opened. */
  isBusy: (address: string) => boolean
  activeCount: number
}

const TransferContext = createContext<TransferApi | null>(null)

export function TransferProvider({ children }: { children: ReactNode }) {
  const [transfers, dispatch] = useReducer(reduce, [])

  useEffect(() => {
    let stop: (() => void) | undefined
    let disposed = false
    void onTransferProgress((p) => {
      dispatch({ type: 'progress', id: p.id, done: p.bytes_done, total: p.bytes_total })
    }).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })
    return () => {
      disposed = true
      stop?.()
    }
  }, [])

  const start = useCallback(async (address: string, given?: string) => {
    const id = given ?? crypto.randomUUID()
    dispatch({
      type: 'add',
      transfer: { id, address, state: 'connecting', done: 0, total: 0, stale: false, error: null },
    })
    try {
      const opened = await shell.open(address, id)
      dispatch({ type: 'finish', id, stale: opened.stale })
      return opened
    } catch (error) {
      dispatch({ type: 'fail', id, error: error instanceof ShellError ? error.code : 'unknown' })
      return null
    }
  }, [])

  const cancel = useCallback((id: string) => {
    void shell.cancel(id)
  }, [])

  const retry = useCallback(
    (id: string) => {
      const failed = transfers.find((t) => t.id === id)
      if (failed === undefined) return
      dispatch({ type: 'dismiss', id })
      void start(failed.address)
    },
    [transfers, start],
  )

  const dismiss = useCallback((id: string) => {
    dispatch({ type: 'dismiss', id })
  }, [])

  const clearFinished = useCallback(() => {
    dispatch({ type: 'clear' })
  }, [])

  const isBusy = useCallback(
    (address: string) => transfers.some((t) => t.address === address && isActive(t)),
    [transfers],
  )

  const activeCount = transfers.filter(isActive).length

  const value = useMemo(
    () => ({ transfers, start, cancel, retry, dismiss, clearFinished, isBusy, activeCount }),
    [transfers, start, cancel, retry, dismiss, clearFinished, isBusy, activeCount],
  )
  return <TransferContext value={value}>{children}</TransferContext>
}

export function useTransfers(): TransferApi {
  const value = useContext(TransferContext)
  if (value === null) throw new Error('useTransfers needs a TransferProvider')
  return value
}

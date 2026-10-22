import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  type ReactNode,
} from 'react'
import { ShellError, onTransferProgress, shell } from '../../shared/ipc.js'

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
  }
}

interface TransferApi {
  transfers: Transfer[]
  start: (address: string) => Promise<void>
  cancel: (id: string) => void
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

  const start = useCallback(async (address: string) => {
    const id = crypto.randomUUID()
    dispatch({
      type: 'add',
      transfer: { id, address, state: 'connecting', done: 0, total: 0, stale: false, error: null },
    })
    try {
      const opened = await shell.open(address, id)
      dispatch({ type: 'finish', id, stale: opened.stale })
    } catch (error) {
      dispatch({ type: 'fail', id, error: error instanceof ShellError ? error.code : 'unknown' })
    }
  }, [])

  const cancel = useCallback((id: string) => {
    void shell.cancel(id)
  }, [])

  const value = useMemo(() => ({ transfers, start, cancel }), [transfers, start, cancel])
  return <TransferContext value={value}>{children}</TransferContext>
}

export function useTransfers(): TransferApi {
  const value = useContext(TransferContext)
  if (value === null) throw new Error('useTransfers needs a TransferProvider')
  return value
}

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  type ReactNode,
} from 'react'
import { shortAddress } from '../../shared/hue.js'
import { onOpenPending, onTabDiscarded, shell } from '../../shared/ipc.js'
import { useTransfers } from '../transfers/model.js'

export type TabKind = 'home' | 'transfers' | 'settings' | 'app'
export type TabState = 'idle' | 'loading' | 'live' | 'discarded' | 'failed'

export interface Tab {
  key: string
  kind: TabKind
  /** `ilmek://…`, for application tabs. */
  address: string | null
  title: string | null
  /** Names the tab's native session, while the tab is live. */
  label: string | null
  /** What the tab's frame loads, while the tab is live. */
  url: string | null
  state: TabState
  transferId: string | null
}

interface Model {
  tabs: Tab[]
  active: string
}

type Action =
  | { type: 'add'; tab: Tab; after?: string }
  | { type: 'patch'; key: string; patch: Partial<Tab> }
  | { type: 'activate'; key: string }
  | { type: 'close'; key: string }
  | { type: 'discarded'; label: string }

export function newTab(kind: TabKind, extra: Partial<Tab> = {}): Tab {
  return {
    key: crypto.randomUUID(),
    kind,
    address: null,
    title: null,
    label: null,
    url: null,
    state: 'idle',
    transferId: null,
    ...extra,
  }
}

export function reduce(model: Model, action: Action): Model {
  switch (action.type) {
    case 'add': {
      const index = action.after === undefined ? -1 : model.tabs.findIndex((t) => t.key === action.after)
      const tabs = [...model.tabs]
      tabs.splice(index === -1 ? tabs.length : index + 1, 0, action.tab)
      return { tabs, active: action.tab.key }
    }
    case 'patch':
      return {
        ...model,
        tabs: model.tabs.map((t) => (t.key === action.key ? { ...t, ...action.patch } : t)),
      }
    case 'activate':
      return model.tabs.some((t) => t.key === action.key) ? { ...model, active: action.key } : model
    case 'close': {
      const index = model.tabs.findIndex((t) => t.key === action.key)
      if (index === -1) return model
      const tabs = model.tabs.filter((t) => t.key !== action.key)
      if (tabs.length === 0) {
        const fresh = newTab('home')
        return { tabs: [fresh], active: fresh.key }
      }
      if (model.active !== action.key) return { tabs, active: model.active }
      return { tabs, active: (tabs[index] ?? tabs[index - 1] ?? tabs[0])?.key ?? model.active }
    }
    case 'discarded':
      return {
        ...model,
        tabs: model.tabs.map((t) =>
          t.label === action.label ? { ...t, label: null, url: null, state: 'discarded' } : t,
        ),
      }
  }
}

interface TabsApi {
  tabs: Tab[]
  active: Tab
  activate: (key: string) => void
  /** A new tab showing the start page. */
  add: () => void
  close: (key: string) => void
  /** Opens `address` in a tab (the start page tab, else a new one) or switches to its tab. */
  open: (address: string) => void
  /** Opens the transfers or settings page, or switches to it. */
  openPage: (kind: 'transfers' | 'settings') => void
  /** Fetches the application again. */
  reload: (key: string) => void
}

const TabsContext = createContext<TabsApi | null>(null)

function initial(): Model {
  const first = newTab('home')
  return { tabs: [first], active: first.key }
}

export function TabsProvider({ children }: { children: ReactNode }) {
  const [model, dispatch] = useReducer(reduce, undefined, initial)
  const { start, cancel } = useTransfers()
  const latest = useRef(model)
  useEffect(() => {
    latest.current = model
  }, [model])

  const active = model.tabs.find((t) => t.key === model.active) ?? model.tabs[0]
  if (active === undefined) throw new Error('there is always a tab')

  /** Runs the transfer for `key` and moves the tab to live or failed. */
  const load = useCallback(
    async (key: string, address: string) => {
      const transferId = crypto.randomUUID()
      dispatch({
        type: 'patch',
        key,
        patch: { kind: 'app', address, title: shortAddress(address), state: 'loading', transferId },
      })
      const opened = await start(address, transferId)
      const current = latest.current.tabs.find((t) => t.key === key)
      if (current?.transferId !== transferId) {
        // The tab was closed or loaded something else meanwhile: drop the finished session.
        if (opened !== null) void shell.closeTab(opened.label)
        return
      }
      if (opened === null) {
        dispatch({ type: 'patch', key, patch: { state: 'failed', transferId: null } })
        return
      }
      dispatch({
        type: 'patch',
        key,
        patch: {
          state: 'live',
          label: opened.label,
          url: opened.url,
          title: opened.title,
          transferId: null,
        },
      })
    },
    [start],
  )

  const open = useCallback(
    (address: string) => {
      const { tabs, active: activeKey } = latest.current
      const existing = tabs.find((t) => t.address === address && t.kind === 'app')
      if (existing !== undefined) {
        dispatch({ type: 'activate', key: existing.key })
        if (existing.state === 'discarded' || existing.state === 'failed') {
          void load(existing.key, address)
        }
        return
      }
      const current = tabs.find((t) => t.key === activeKey)
      if (current?.kind === 'home') {
        void load(current.key, address)
        return
      }
      const tab = newTab('app', { address, title: shortAddress(address), state: 'loading' })
      dispatch({ type: 'add', tab, after: activeKey })
      void load(tab.key, address)
    },
    [load],
  )

  const reload = useCallback(
    (key: string) => {
      const tab = latest.current.tabs.find((t) => t.key === key)
      if (tab?.address == null) return
      if (tab.label !== null) void shell.closeTab(tab.label)
      void load(key, tab.address)
    },
    [load],
  )

  const close = useCallback(
    (key: string) => {
      const tab = latest.current.tabs.find((t) => t.key === key)
      if (tab?.label != null) void shell.closeTab(tab.label)
      if (tab?.transferId != null) cancel(tab.transferId)
      dispatch({ type: 'close', key })
    },
    [cancel],
  )

  const add = useCallback(() => {
    dispatch({ type: 'add', tab: newTab('home'), after: latest.current.active })
  }, [])

  const openPage = useCallback((kind: 'transfers' | 'settings') => {
    const existing = latest.current.tabs.find((t) => t.kind === kind)
    if (existing !== undefined) dispatch({ type: 'activate', key: existing.key })
    else dispatch({ type: 'add', tab: newTab(kind), after: latest.current.active })
  }, [])

  const activate = useCallback((key: string) => {
    dispatch({ type: 'activate', key })
  }, [])

  // Tell the shell which tab is on screen (it discards the longest-hidden ones first), and bring
  // a discarded tab back when it is chosen.
  useEffect(() => {
    void shell.showTab(active.kind === 'app' && active.state === 'live' ? active.label : null)
    if (active.kind === 'app' && active.state === 'discarded' && active.address !== null) {
      void load(active.key, active.address)
    }
  }, [active.key, active.kind, active.state, active.label, active.address, load])

  // The native side gave up a background tab's memory.
  useEffect(() => {
    let stop: (() => void) | undefined
    let disposed = false
    void onTabDiscarded((label) => {
      dispatch({ type: 'discarded', label })
    }).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })
    return () => {
      disposed = true
      stop?.()
    }
  }, [])

  // Links from a browser or the command line arrive as queued addresses.
  useEffect(() => {
    let stop: (() => void) | undefined
    let disposed = false
    const drain = () => {
      void shell.pending().then(
        (addresses) => {
          for (const address of addresses) open(address)
        },
        () => undefined,
      )
    }
    drain()
    void onOpenPending(drain).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })
    return () => {
      disposed = true
      stop?.()
    }
  }, [open])

  // Browser shortcuts: new tab, close tab, next/previous tab.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (!(event.ctrlKey || event.metaKey)) return
      const { tabs, active: activeKey } = latest.current
      const index = tabs.findIndex((t) => t.key === activeKey)
      const key = event.key.toLowerCase()
      if (key === 't') {
        event.preventDefault()
        add()
      } else if (key === 'w') {
        event.preventDefault()
        close(activeKey)
      } else if (event.key === 'Tab') {
        event.preventDefault()
        const step = event.shiftKey ? -1 : 1
        const next = tabs[(index + step + tabs.length) % tabs.length]
        if (next !== undefined) activate(next.key)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('keydown', onKey)
    }
  }, [add, close, activate])

  const value = useMemo(
    () => ({ tabs: model.tabs, active, activate, add, close, open, openPage, reload }),
    [model.tabs, active, activate, add, close, open, openPage, reload],
  )
  return <TabsContext value={value}>{children}</TabsContext>
}

export function useTabs(): TabsApi {
  const value = useContext(TabsContext)
  if (value === null) throw new Error('useTabs needs a TabsProvider')
  return value
}

import { useCallback, useEffect, useState } from 'react'
import { shell, type History } from '../../shared/ipc.js'

export interface HistoryApi {
  history: History
  reload: () => void
  isBookmarked: (address: string) => boolean
  setBookmark: (address: string, on: boolean) => void
}

/** Bookmarks and recently opened addresses, kept by the native shell. */
export function useHistory(): HistoryApi {
  const [history, setHistory] = useState<History>({ visited: [], bookmarks: [] })

  const reload = useCallback(() => {
    void shell.history().then(setHistory, () => undefined)
  }, [])

  useEffect(() => {
    reload()
  }, [reload])

  const isBookmarked = useCallback(
    (address: string) => history.bookmarks.includes(address),
    [history.bookmarks],
  )

  const setBookmark = useCallback(
    (address: string, on: boolean) => {
      void shell.bookmark(address, on).then(reload, () => undefined)
    },
    [reload],
  )

  return { history, reload, isBookmarked, setBookmark }
}

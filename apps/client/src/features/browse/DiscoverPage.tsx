import { useCallback, useEffect, useState } from 'react'
import { useT } from '../../shared/i18n/index.js'
import { shell, type History } from '../../shared/ipc.js'
import { useTransfers } from '../transfers/model.js'
import { AddressBar } from './AddressBar.js'
import { Catalog } from './Catalog.js'

function AddressList({ title, addresses }: { title: string; addresses: string[] }) {
  const t = useT()
  const { start } = useTransfers()
  return (
    <section>
      <h3>{title}</h3>
      {addresses.length === 0 ? (
        <p>{t('history.none')}</p>
      ) : (
        <ul className="plain">
          {addresses.map((address) => (
            <li key={address}>
              <button
                type="button"
                className="link"
                onClick={() => {
                  void start(address)
                }}
              >
                {address}
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

export function DiscoverPage() {
  const t = useT()
  const [history, setHistory] = useState<History>({ visited: [], bookmarks: [] })
  const { transfers } = useTransfers()

  const reload = useCallback(() => {
    void shell.history().then(setHistory, () => undefined)
  }, [])

  useEffect(() => {
    reload()
  }, [reload, transfers.length])

  return (
    <>
      <AddressBar
        bookmarked={(address) => history.bookmarks.includes(address)}
        onBookmark={(address, on) => {
          void shell.bookmark(address, on).then(reload, () => undefined)
        }}
      />
      <div className="columns">
        <Catalog />
        <aside>
          <AddressList title={t('history.bookmarks')} addresses={history.bookmarks} />
          <AddressList title={t('history.recent')} addresses={history.visited} />
        </aside>
      </div>
    </>
  )
}

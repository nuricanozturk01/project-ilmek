import { useCallback, useEffect, useState, type SyntheticEvent } from 'react'
import { errorText, useT } from '../../shared/i18n/index.js'
import { ShellError, shell, type CatalogEntry } from '../../shared/ipc.js'
import { useTransfers } from '../transfers/model.js'

export function Catalog() {
  const t = useT()
  const { start } = useTransfers()
  const [query, setQuery] = useState('')
  const [entries, setEntries] = useState<CatalogEntry[]>([])
  const [next, setNext] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [searched, setSearched] = useState(false)

  const load = useCallback(async (text: string, cursor: string | null) => {
    try {
      const page = await shell.catalog(text, cursor)
      setEntries((current) => (cursor === null ? page.entries : [...current, ...page.entries]))
      setNext(page.next_cursor)
      setSearched(true)
      setError(null)
    } catch (failure) {
      setError(failure instanceof ShellError ? failure.code : 'unknown')
    }
  }, [])

  useEffect(() => {
    let live = true
    void shell.catalog('', null).then(
      (page) => {
        if (!live) return
        setEntries(page.entries)
        setNext(page.next_cursor)
        setSearched(true)
      },
      (failure: unknown) => {
        if (live) setError(failure instanceof ShellError ? failure.code : 'unknown')
      },
    )
    return () => {
      live = false
    }
  }, [])

  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault()
    void load(query.trim(), null)
  }

  return (
    <section aria-labelledby="catalog-title">
      <h2 id="catalog-title">{t('catalog.title')}</h2>
      <form className="row" role="search" onSubmit={submit}>
        <label htmlFor="catalog-query">{t('catalog.search')}</label>
        <input
          id="catalog-query"
          value={query}
          onChange={(event) => {
            setQuery(event.target.value)
          }}
          placeholder={t('catalog.searchPlaceholder')}
        />
        <button type="submit">{t('catalog.search')}</button>
      </form>
      {error !== null ? <p role="alert">{errorText(t, error)}</p> : null}
      {searched && entries.length === 0 && error === null ? <p>{t('catalog.empty')}</p> : null}
      <ul className="plain">
        {entries.map((entry) => (
          <li key={entry.address} className="card">
            <div>
              <strong>{entry.name}</strong> <code>{entry.address}</code>
            </div>
            <p>{entry.description}</p>
            <p className="meta">
              {t('catalog.version', { version: entry.version })} ·{' '}
              {t(`catalog.availability.${entry.availability}`)}
              {entry.cached ? ` · ${t('catalog.cached')}` : ''}
            </p>
            <button
              type="button"
              onClick={() => {
                void start(entry.address)
              }}
            >
              {t('address.open')}
            </button>
          </li>
        ))}
      </ul>
      {next !== null ? (
        <button
          type="button"
          onClick={() => {
            void load(query.trim(), next)
          }}
        >
          {t('catalog.more')}
        </button>
      ) : null}
    </section>
  )
}

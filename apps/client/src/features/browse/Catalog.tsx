import { useCallback, useEffect, useState, type SyntheticEvent } from 'react'
import { hueClass, initials } from '../../shared/hue.js'
import { errorText, useT, type MessageKey } from '../../shared/i18n/index.js'
import { ShellError, shell, type CatalogEntry } from '../../shared/ipc.js'
import { Icon, Spinner } from '../../shared/ui/Icon.js'
import { useTabs } from '../tabs/model.js'
import { useTransfers } from '../transfers/model.js'

function Card({ entry }: { entry: CatalogEntry }) {
  const t = useT()
  const { isBusy } = useTransfers()
  const { open } = useTabs()
  const busy = isBusy(entry.address)
  return (
    <li className="app-card">
      <div className="card-top">
        <span className={`avatar ${hueClass(entry.address)}`} aria-hidden="true">
          {initials(entry.name)}
        </span>
        <div className="card-title">
          <strong>{entry.name}</strong>
          <code>{entry.address.replace(/^ilmek:\/\//, '')}</code>
        </div>
      </div>
      <p className="card-text">{entry.description}</p>
      <div className="card-meta">
        <span className={`badge badge-${entry.availability}`}>
          <span className="dot" aria-hidden="true" />
          {t(`catalog.availability.${entry.availability}` as MessageKey)}
        </span>
        <span className="meta">{t('catalog.version', { version: entry.version })}</span>
        {entry.cached ? <span className="badge badge-neutral">{t('catalog.cached')}</span> : null}
      </div>
      <button
        type="button"
        className="primary card-action"
        disabled={busy}
        onClick={() => {
          open(entry.address)
        }}
      >
        {busy ? (
          <>
            <Spinner />
            {t('address.opening')}
          </>
        ) : (
          t('address.open')
        )}
      </button>
    </li>
  )
}

function Skeletons() {
  return (
    <ul className="grid" aria-hidden="true">
      {[0, 1, 2].map((n) => (
        <li key={n} className="app-card skeleton">
          <span className="sk sk-avatar" />
          <span className="sk sk-line" />
          <span className="sk sk-line sk-short" />
        </li>
      ))}
    </ul>
  )
}

export function Catalog() {
  const t = useT()
  const [query, setQuery] = useState('')
  const [entries, setEntries] = useState<CatalogEntry[]>([])
  const [next, setNext] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [searched, setSearched] = useState('')

  const load = useCallback(async (text: string, cursor: string | null) => {
    setLoading(true)
    try {
      const page = await shell.catalog(text, cursor)
      setEntries((current) => (cursor === null ? page.entries : [...current, ...page.entries]))
      setNext(page.next_cursor)
      setSearched(text)
      setError(null)
    } catch (failure) {
      setError(failure instanceof ShellError ? failure.code : 'unknown')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    let live = true
    void shell.catalog('', null).then(
      (page) => {
        if (!live) return
        setEntries(page.entries)
        setNext(page.next_cursor)
        setLoading(false)
      },
      (failure: unknown) => {
        if (!live) return
        setError(failure instanceof ShellError ? failure.code : 'unknown')
        setLoading(false)
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

  const nothing = !loading && error === null && entries.length === 0

  return (
    <section aria-labelledby="catalog-title" className="section">
      <div className="section-head">
        <h2 id="catalog-title">{t('catalog.title')}</h2>
        <form className="search" role="search" onSubmit={submit}>
          <label htmlFor="catalog-query" className="sr-only">
            {t('catalog.search')}
          </label>
          <span className="search-icon">
            <Icon name="search" />
          </span>
          <input
            id="catalog-query"
            type="search"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value)
            }}
            placeholder={t('catalog.searchPlaceholder')}
          />
          <button type="submit">{t('catalog.search')}</button>
        </form>
      </div>

      {error !== null ? (
        <div className="notice notice-error" role="alert">
          <Icon name="alert" />
          <span>{errorText(t, error)}</span>
          <button
            type="button"
            onClick={() => {
              void load(query.trim(), null)
            }}
          >
            {t('catalog.retry')}
          </button>
        </div>
      ) : null}

      {loading && entries.length === 0 && error === null ? (
        <>
          <p className="sr-only" role="status">
            {t('catalog.loading')}
          </p>
          <Skeletons />
        </>
      ) : null}

      {nothing ? (
        <div className="empty">
          <span className="empty-art">
            <Icon name="box" />
          </span>
          <h3>{searched === '' ? t('catalog.empty') : t('catalog.noMatch')}</h3>
          {searched === '' ? <p>{t('catalog.emptyHint')}</p> : null}
        </div>
      ) : null}

      {entries.length > 0 ? (
        <ul className="grid">
          {entries.map((entry) => (
            <Card key={entry.address} entry={entry} />
          ))}
        </ul>
      ) : null}

      {next !== null ? (
        <button
          type="button"
          className="more"
          disabled={loading}
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

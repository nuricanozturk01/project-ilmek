import { useCallback, useEffect, useState } from 'react'
import { DiscoverPage } from '../features/browse/DiscoverPage.js'
import { JoinPage } from '../features/join/JoinPage.js'
import { SettingsPage } from '../features/settings/SettingsPage.js'
import { TransferProvider } from '../features/transfers/model.js'
import { TransfersPage } from '../features/transfers/TransfersPage.js'
import { errorText, useT } from '../shared/i18n/index.js'
import { ShellError, shell, type ShellState } from '../shared/ipc.js'

type Tab = 'discover' | 'transfers' | 'settings'
const TABS: readonly Tab[] = ['discover', 'transfers', 'settings']

export function App({ locale }: { locale: string }) {
  const t = useT()
  const [state, setState] = useState<ShellState | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [tab, setTab] = useState<Tab>('discover')

  const load = useCallback(() => {
    shell.state().then(
      (next) => {
        setState(next)
        setError(null)
      },
      (failure: unknown) => {
        setError(failure instanceof ShellError ? failure.code : 'unknown')
      },
    )
  }, [])

  useEffect(() => {
    let live = true
    shell.state().then(
      (next) => {
        if (live) setState(next)
      },
      (failure: unknown) => {
        if (live) setError(failure instanceof ShellError ? failure.code : 'unknown')
      },
    )
    return () => {
      live = false
    }
  }, [])

  if (error !== null) return <p role="alert">{errorText(t, error)}</p>
  if (state === null) return null
  if (!state.joined) return <JoinPage onJoined={load} />

  return (
    <TransferProvider>
      <header className="bar">
        <h1>{t('appName')}</h1>
        <nav aria-label={t('appName')}>
          {TABS.map((name) => (
            <button
              key={name}
              type="button"
              aria-current={tab === name ? 'page' : undefined}
              onClick={() => {
                setTab(name)
              }}
            >
              {t(`nav.${name}`)}
            </button>
          ))}
        </nav>
      </header>
      <main>
        {tab === 'discover' ? <DiscoverPage /> : null}
        {tab === 'transfers' ? <TransfersPage locale={locale} /> : null}
        {tab === 'settings' ? <SettingsPage initial={state.status} locale={locale} /> : null}
      </main>
    </TransferProvider>
  )
}

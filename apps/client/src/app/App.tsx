import { useCallback, useEffect, useState } from 'react'
import { AddressBar } from '../features/browse/AddressBar.js'
import { DiscoverPage } from '../features/browse/DiscoverPage.js'
import { useHistory, type HistoryApi } from '../features/browse/useHistory.js'
import { JoinPage } from '../features/join/JoinPage.js'
import { SettingsPage } from '../features/settings/SettingsPage.js'
import { TabsProvider, useTabs } from '../features/tabs/model.js'
import { TabStrip } from '../features/tabs/TabStrip.js'
import { Frames } from '../features/tabs/Frames.js'
import { AppPlaceholder } from '../features/tabs/Viewport.js'
import { TransferProvider, useTransfers } from '../features/transfers/model.js'
import { TransfersPage } from '../features/transfers/TransfersPage.js'
import { errorText, useT } from '../shared/i18n/index.js'
import { ShellError, shell, type ShellState, type Status } from '../shared/ipc.js'
import { Icon, Spinner } from '../shared/ui/Icon.js'

function ConnectionChip({ status }: { status: Status | null }) {
  const t = useT()
  const { openPage } = useTabs()
  if (status === null) return null
  const label = status.registry_reachable ? t('status.connected') : t('status.disconnected')
  return (
    <button
      type="button"
      className={status.registry_reachable ? 'chip chip-ok' : 'chip chip-down'}
      title={`${t('status.details')}: ${status.registry}`}
      onClick={() => {
        openPage('settings')
      }}
    >
      <span className="dot" aria-hidden="true" />
      {label}
    </button>
  )
}

function Page({
  locale,
  status,
  history,
}: {
  locale: string
  status: Status | null
  history: HistoryApi
}) {
  const { active } = useTabs()
  return (
    <div className="viewport" id="tabpanel">
      {active.kind === 'app' && active.state === 'live' ? null : (
        <main className="page">
          {active.kind === 'home' ? <DiscoverPage history={history} /> : null}
          {active.kind === 'transfers' ? <TransfersPage locale={locale} /> : null}
          {active.kind === 'settings' ? <SettingsPage initial={status} locale={locale} /> : null}
          {active.kind === 'app' ? <AppPlaceholder tab={active} /> : null}
        </main>
      )}
      <Frames />
    </div>
  )
}

function Chrome({ locale, status }: { locale: string; status: Status | null }) {
  const t = useT()
  const { openPage } = useTabs()
  const { activeCount, transfers } = useTransfers()
  const history = useHistory()
  const reloadHistory = history.reload

  // Opening something adds a transfer; the recent list changes with it.
  useEffect(() => {
    reloadHistory()
  }, [reloadHistory, transfers.length])

  return (
    <div className="browser">
      <div className="chrome">
        <div className="titlebar">
          <span className="brand" aria-hidden="true">
            <Icon name="logo" />
          </span>
          <TabStrip />
          <span className="tools">
            <button
              type="button"
              className="icon-button"
              aria-label={t('nav.transfers')}
              title={t('nav.transfers')}
              onClick={() => {
                openPage('transfers')
              }}
            >
              <Icon name="download" />
              {activeCount > 0 ? (
                <span className="badge-count" aria-hidden="true">
                  {activeCount}
                </span>
              ) : null}
            </button>
            <button
              type="button"
              className="icon-button"
              aria-label={t('nav.settings')}
              title={t('nav.settings')}
              onClick={() => {
                openPage('settings')
              }}
            >
              <Icon name="settings" />
            </button>
          </span>
        </div>
        <header className="toolbar">
          <AddressBar bookmarked={history.isBookmarked} onBookmark={history.setBookmark} />
          <ConnectionChip status={status} />
        </header>
      </div>
      <Page locale={locale} status={status} history={history} />
    </div>
  )
}

export function App({ locale }: { locale: string }) {
  const t = useT()
  const [state, setState] = useState<ShellState | null>(null)
  const [error, setError] = useState<string | null>(null)

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

  if (error !== null) {
    return (
      <main className="splash">
        <p role="alert" className="notice notice-error">
          {errorText(t, error)}
        </p>
      </main>
    )
  }
  if (state === null) {
    return (
      <main className="splash">
        <Spinner />
      </main>
    )
  }
  if (!state.joined) return <JoinPage onJoined={load} />

  return (
    <TransferProvider>
      <TabsProvider>
        <Chrome locale={locale} status={state.status} />
      </TabsProvider>
    </TransferProvider>
  )
}

import { useCallback, useEffect, useState } from 'react'
import { formatBytes } from '../../shared/format.js'
import { errorText, useT } from '../../shared/i18n/index.js'
import { ShellError, shell, type CachedApp, type Status } from '../../shared/ipc.js'

export function SettingsPage({ initial, locale }: { initial: Status | null; locale: string }) {
  const t = useT()
  const [status, setStatus] = useState<Status | null>(initial)
  const [cached, setCached] = useState<CachedApp[]>([])
  const [report, setReport] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const loadCache = useCallback(() => {
    void shell.cached().then(setCached, () => undefined)
  }, [])

  useEffect(() => {
    loadCache()
  }, [loadCache])

  async function refresh() {
    setError(null)
    try {
      setStatus(await shell.refresh())
    } catch (failure) {
      setError(failure instanceof ShellError ? failure.code : 'unknown')
    }
  }

  async function remove(address: string) {
    setError(null)
    try {
      await shell.remove(address)
      loadCache()
    } catch (failure) {
      setError(failure instanceof ShellError ? failure.code : 'unknown')
    }
  }

  return (
    <section aria-labelledby="settings-title">
      <h2 id="settings-title">{t('settings.title')}</h2>
      {status !== null ? (
        <>
          <h3>{t('settings.status')}</h3>
          <dl>
            <dt>{t('settings.community')}</dt>
            <dd>
              <code>{status.community_id}</code>
            </dd>
            <dt>{t('settings.device')}</dt>
            <dd>
              <code>{status.peer_id}</code>
            </dd>
            <dt>{t('settings.registry')}</dt>
            <dd>
              {status.registry} —{' '}
              {status.registry_reachable ? t('settings.reachable') : t('settings.unreachable')}
            </dd>
          </dl>
          <button type="button" onClick={() => void refresh()}>
            {t('settings.refresh')}
          </button>
        </>
      ) : null}
      {error !== null ? <p role="alert">{errorText(t, error)}</p> : null}

      <h3>{t('settings.cache')}</h3>
      {cached.length === 0 ? (
        <p>{t('settings.cacheEmpty')}</p>
      ) : (
        <ul className="plain">
          {cached.map((app) => (
            <li key={`${app.address}-${app.version}`} className="card">
              <code>{app.address}</code> · {app.version} · {formatBytes(app.bytes, locale)}{' '}
              <button
                type="button"
                onClick={() => {
                  void remove(app.address)
                }}
              >
                {t('settings.remove')}
              </button>
            </li>
          ))}
        </ul>
      )}

      <button
        type="button"
        onClick={() => {
          void shell.diagnostics().then(setReport, () => undefined)
        }}
      >
        {t('settings.diagnostics')}
      </button>
      <p className="meta">{t('settings.diagnosticsNote')}</p>
      {report !== null ? <pre>{report}</pre> : null}
    </section>
  )
}

import { useCallback, useEffect, useState } from 'react'
import { formatBytes } from '../../shared/format.js'
import { hueClass, initials, shortAddress } from '../../shared/hue.js'
import { errorText, useT } from '../../shared/i18n/index.js'
import { ShellError, shell, type CachedApp, type Status } from '../../shared/ipc.js'
import { Icon } from '../../shared/ui/Icon.js'

function CopyButton({ text }: { text: string }) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  return (
    <button
      type="button"
      className="icon-button"
      aria-label={`${t('settings.copy')}: ${text}`}
      title={copied ? t('settings.copied') : t('settings.copy')}
      onClick={() => {
        void navigator.clipboard.writeText(text).then(
          () => {
            setCopied(true)
            window.setTimeout(() => {
              setCopied(false)
            }, 1500)
          },
          () => undefined,
        )
      }}
    >
      <Icon name={copied ? 'check' : 'copy'} />
    </button>
  )
}

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
    <section aria-labelledby="settings-title" className="stack-lg">
      <div className="page-head">
        <h1 id="settings-title">{t('settings.title')}</h1>
      </div>

      {status !== null ? (
        <section className="panel" aria-labelledby="status-title">
          <div className="panel-head">
            <h2 id="status-title">{t('settings.status')}</h2>
            <span className={status.registry_reachable ? 'badge badge-recent_heartbeat' : 'badge badge-stale'}>
              <span className="dot" aria-hidden="true" />
              {status.registry_reachable ? t('settings.reachable') : t('settings.unreachable')}
            </span>
          </div>
          <dl className="facts">
            <div>
              <dt>{t('settings.registry')}</dt>
              <dd>
                <code>{status.registry}</code>
              </dd>
            </div>
            <div>
              <dt>{t('settings.community')}</dt>
              <dd>
                <code>{status.community_id}</code>
                <CopyButton text={status.community_id} />
              </dd>
            </div>
            <div>
              <dt>{t('settings.device')}</dt>
              <dd>
                <code>{status.peer_id}</code>
                <CopyButton text={status.peer_id} />
              </dd>
            </div>
          </dl>
          <button type="button" onClick={() => void refresh()}>
            <Icon name="refresh" />
            {t('settings.refresh')}
          </button>
        </section>
      ) : null}

      {error !== null ? (
        <p role="alert" className="notice notice-error">
          <Icon name="alert" />
          <span>{errorText(t, error)}</span>
        </p>
      ) : null}

      <section className="panel" aria-labelledby="cache-title">
        <div className="panel-head">
          <h2 id="cache-title">{t('settings.cache')}</h2>
        </div>
        {cached.length === 0 ? (
          <p className="meta">{t('settings.cacheEmpty')}</p>
        ) : (
          <ul className="rows">
            {cached.map((app) => (
              <li key={`${app.address}-${app.version}`} className="row-item">
                <span className={`avatar avatar-sm ${hueClass(app.address)}`} aria-hidden="true">
                  {initials(shortAddress(app.address))}
                </span>
                <span className="row-main">
                  <code>{app.address}</code>
                  <span className="meta">
                    {app.version} · <span>{formatBytes(app.bytes, locale)}</span>
                  </span>
                </span>
                <button
                  type="button"
                  onClick={() => {
                    void remove(app.address)
                  }}
                >
                  <Icon name="trash" />
                  {t('settings.remove')}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
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
    </section>
  )
}

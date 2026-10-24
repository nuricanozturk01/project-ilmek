import { useState, type SyntheticEvent } from 'react'
import { errorText, useT } from '../../shared/i18n/index.js'
import { ShellError, shell } from '../../shared/ipc.js'
import { Icon, Spinner } from '../../shared/ui/Icon.js'

export function JoinPage({ onJoined }: { onJoined: () => void }) {
  const t = useT()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault()
    const data = new FormData(event.currentTarget)
    const field = (name: string) => {
      const value = data.get(name)
      return typeof value === 'string' ? value : ''
    }
    setBusy(true)
    setError(null)
    try {
      await shell.join(field('profile') || null, field('registry') || null)
      onJoined()
    } catch (failure) {
      setError(failure instanceof ShellError ? failure.code : 'unknown')
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="splash">
      <section className="join-card" aria-labelledby="join-title">
        <span className="join-mark">
          <Icon name="logo" />
        </span>
        <h1 id="join-title">{t('join.title')}</h1>
        <p className="lead">{t('join.intro')}</p>
        <form onSubmit={(event) => void submit(event)}>
          <label>
            <span>{t('join.registry')}</span>
            <input name="registry" placeholder="192.168.1.10:47401" spellCheck={false} />
            <small className="meta">{t('join.registryHint')}</small>
          </label>
          <details>
            <summary>{t('join.advanced')}</summary>
            <label>
              <span>{t('join.profile')}</span>
              <textarea name="profile" rows={6} spellCheck={false} />
            </label>
          </details>
          <button type="submit" className="primary large" disabled={busy}>
            {busy ? (
              <>
                <Spinner />
                {t('join.working')}
              </>
            ) : (
              t('join.submit')
            )}
          </button>
          {busy ? (
            <p className="meta" role="status">
              {t('join.workingHint')}
            </p>
          ) : null}
          {error !== null ? (
            <p role="alert" className="notice notice-error">
              <Icon name="alert" />
              <span>{errorText(t, error)}</span>
            </p>
          ) : null}
        </form>
      </section>
    </main>
  )
}

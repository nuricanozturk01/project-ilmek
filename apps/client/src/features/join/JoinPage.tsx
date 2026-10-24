import { useState, type SyntheticEvent } from 'react'
import { errorText, useT } from '../../shared/i18n/index.js'
import { ShellError, shell } from '../../shared/ipc.js'

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
    <main className="narrow">
      <h1>{t('join.title')}</h1>
      <p>{t('join.intro')}</p>
      <form onSubmit={(event) => void submit(event)}>
        <label>
          {t('join.registry')}
          <input name="registry" placeholder="192.168.1.10:47401" spellCheck={false} />
        </label>
        <details>
          <summary>{t('join.advanced')}</summary>
          <label>
            {t('join.profile')}
            <textarea name="profile" rows={6} spellCheck={false} />
          </label>
        </details>
        <button type="submit" disabled={busy}>
          {busy ? t('join.working') : t('join.submit')}
        </button>
        {error !== null ? <p role="alert">{errorText(t, error)}</p> : null}
      </form>
    </main>
  )
}

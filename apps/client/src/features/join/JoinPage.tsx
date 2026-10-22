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
      await shell.join(field('profile'), field('fingerprint'), field('token'), field('name'))
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
          {t('join.profile')}
          <textarea name="profile" rows={6} required spellCheck={false} />
        </label>
        <label>
          {t('join.fingerprint')}
          <input name="fingerprint" required autoComplete="off" spellCheck={false} />
        </label>
        <label>
          {t('join.token')}
          <input name="token" type="password" required autoComplete="off" spellCheck={false} />
        </label>
        <label>
          {t('join.name')}
          <input name="name" required maxLength={80} autoComplete="off" />
        </label>
        <button type="submit" disabled={busy}>
          {busy ? t('join.working') : t('join.submit')}
        </button>
        {error !== null ? <p role="alert">{errorText(t, error)}</p> : null}
      </form>
    </main>
  )
}

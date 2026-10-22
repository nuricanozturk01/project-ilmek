import { useState, type SyntheticEvent } from 'react'
import { useT } from '../../shared/i18n/index.js'
import { useTransfers } from '../transfers/model.js'

export function AddressBar({
  bookmarked,
  onBookmark,
}: {
  bookmarked: (address: string) => boolean
  onBookmark: (address: string, on: boolean) => void
}) {
  const t = useT()
  const { start } = useTransfers()
  const [address, setAddress] = useState('')
  const trimmed = address.trim()

  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault()
    if (trimmed !== '') void start(trimmed)
  }

  return (
    <form className="address" role="search" onSubmit={submit}>
      <label htmlFor="address">{t('address.label')}</label>
      <input
        id="address"
        value={address}
        onChange={(event) => {
          setAddress(event.target.value)
        }}
        placeholder={t('address.placeholder')}
        autoComplete="off"
        spellCheck={false}
        inputMode="url"
      />
      <button type="submit">{t('address.open')}</button>
      {trimmed !== '' ? (
        <button
          type="button"
          onClick={() => {
            onBookmark(trimmed, !bookmarked(trimmed))
          }}
        >
          {bookmarked(trimmed) ? t('address.unbookmark') : t('address.bookmark')}
        </button>
      ) : null}
    </form>
  )
}

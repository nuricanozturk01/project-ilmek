import { useEffect, useRef, useState, type SyntheticEvent } from 'react'
import { normalizeAddress } from '../../shared/hue.js'
import { useT } from '../../shared/i18n/index.js'
import { Icon } from '../../shared/ui/Icon.js'
import { useTabs } from '../tabs/model.js'

export function AddressBar({
  bookmarked,
  onBookmark,
}: {
  bookmarked: (address: string) => boolean
  onBookmark: (address: string, on: boolean) => void
}) {
  const t = useT()
  const { open, active } = useTabs()
  // The bar shows the address of the tab being looked at, as in a browser, and is free text
  // while it is being edited.
  const shown = active.kind === 'app' ? (active.address ?? '') : ''
  const [address, setAddress] = useState(shown)
  const [source, setSource] = useState(`${active.key}|${shown}`)
  const input = useRef<HTMLInputElement>(null)
  const full = normalizeAddress(address)

  // Switching tabs replaces what was typed with that tab's address (adjusting state while
  // rendering, as React recommends, instead of in an effect).
  const current = `${active.key}|${shown}`
  if (source !== current) {
    setSource(current)
    setAddress(shown)
  }

  // Ctrl+L / Cmd+L, as in every browser.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'l') {
        event.preventDefault()
        input.current?.focus()
        input.current?.select()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('keydown', onKey)
    }
  }, [])

  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault()
    if (full === '') return
    open(full)
  }

  const marked = full !== '' && bookmarked(full)

  return (
    <form className="address" role="search" onSubmit={submit} title={t('address.shortcut')}>
      <label htmlFor="address" className="sr-only">
        {t('address.label')}
      </label>
      <span className="address-icon">
        <Icon name="globe" />
      </span>
      <input
        id="address"
        ref={input}
        value={address}
        onChange={(event) => {
          setAddress(event.target.value)
        }}
        onFocus={(event) => {
          event.currentTarget.select()
        }}
        placeholder={t('address.placeholder')}
        autoComplete="off"
        spellCheck={false}
        inputMode="url"
      />
      {full !== '' ? (
        <button
          type="button"
          className="icon-button"
          aria-pressed={marked}
          aria-label={marked ? t('address.unbookmark') : t('address.bookmark')}
          title={marked ? t('address.unbookmark') : t('address.bookmark')}
          onClick={() => {
            onBookmark(full, !marked)
          }}
        >
          <Icon name="star" filled={marked} />
        </button>
      ) : null}
      <button type="submit" className="primary" disabled={full === ''}>
        {t('address.open')}
      </button>
    </form>
  )
}

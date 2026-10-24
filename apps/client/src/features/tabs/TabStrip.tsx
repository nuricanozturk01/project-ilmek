import type { KeyboardEvent } from 'react'
import { hueClass, initials } from '../../shared/hue.js'
import { useT } from '../../shared/i18n/index.js'
import { Icon, Spinner, type IconName } from '../../shared/ui/Icon.js'
import { useTabs, type Tab } from './model.js'

const PAGE_ICON: Record<'home' | 'transfers' | 'settings', IconName> = {
  home: 'compass',
  transfers: 'download',
  settings: 'settings',
}

export function tabTitle(tab: Tab, t: ReturnType<typeof useT>): string {
  if (tab.kind === 'home') return t('tabs.newTab')
  if (tab.kind === 'transfers') return t('nav.transfers')
  if (tab.kind === 'settings') return t('nav.settings')
  return tab.title ?? tab.address ?? t('tabs.newTab')
}

function Favicon({ tab }: { tab: Tab }) {
  if (tab.kind !== 'app') {
    return (
      <span className="tab-icon" aria-hidden="true">
        <Icon name={PAGE_ICON[tab.kind]} />
      </span>
    )
  }
  if (tab.state === 'loading') {
    return (
      <span className="tab-icon" aria-hidden="true">
        <Spinner />
      </span>
    )
  }
  return (
    <span
      className={`tab-avatar ${hueClass(tab.address ?? '')}${tab.state === 'discarded' ? ' faded' : ''}`}
      aria-hidden="true"
    >
      {initials(tab.title ?? tab.address ?? '')}
    </span>
  )
}

export function TabStrip() {
  const t = useT()
  const { tabs, active, activate, close, add } = useTabs()

  function onKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return
    const index = tabs.findIndex((tab) => tab.key === active.key)
    const step = event.key === 'ArrowRight' ? 1 : -1
    const next = tabs[(index + step + tabs.length) % tabs.length]
    if (next !== undefined) {
      event.preventDefault()
      activate(next.key)
    }
  }

  return (
    <div className="tabstrip">
      <div className="tabs" role="tablist" aria-label={t('tabs.list')}>
        {tabs.map((tab) => {
          const selected = tab.key === active.key
          return (
            <div
              key={tab.key}
              className={selected ? 'tab tab-active' : 'tab'}
              role="presentation"
              onAuxClick={(event) => {
                if (event.button === 1) close(tab.key)
              }}
            >
              <button
                type="button"
                role="tab"
                className="tab-button"
                aria-selected={selected}
                tabIndex={selected ? 0 : -1}
                onKeyDown={onKeyDown}
                title={tabTitle(tab, t)}
                onClick={() => {
                  activate(tab.key)
                }}
              >
                <Favicon tab={tab} />
                <span className="tab-title">{tabTitle(tab, t)}</span>
              </button>
              <button
                type="button"
                className="tab-close"
                aria-label={`${t('tabs.close')}: ${tabTitle(tab, t)}`}
                title={t('tabs.close')}
                onClick={() => {
                  close(tab.key)
                }}
              >
                <Icon name="x" />
              </button>
            </div>
          )
        })}
      </div>
      <button
        type="button"
        className="icon-button new-tab"
        aria-label={t('tabs.new')}
        title={`${t('tabs.new')} (Ctrl+T)`}
        onClick={add}
      >
        <Icon name="plus" />
      </button>
    </div>
  )
}

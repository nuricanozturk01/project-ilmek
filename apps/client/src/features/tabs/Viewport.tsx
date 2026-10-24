import { shortAddress } from '../../shared/hue.js'
import { errorText, useT } from '../../shared/i18n/index.js'
import { Icon, Spinner } from '../../shared/ui/Icon.js'
import { useTransfers } from '../transfers/model.js'
import { useTabs, type Tab } from './model.js'

/** What the content area shows for an application tab that is not (yet) live. */
export function AppPlaceholder({ tab }: { tab: Tab }) {
  const t = useT()
  const { transfers, cancel } = useTransfers()
  const { close, reload } = useTabs()
  const transfer = transfers.find((item) => item.id === tab.transferId)
  const failed = transfers.find((item) => item.address === tab.address && item.state === 'failed')
  const percent =
    transfer !== undefined && transfer.total > 0
      ? Math.round((transfer.done / transfer.total) * 100)
      : null
  const address = shortAddress(tab.address ?? '')

  if (tab.state === 'failed') {
    return (
      <div className="viewport-message" role="alert">
        <span className="empty-art empty-art-error">
          <Icon name="alert" />
        </span>
        <h2>{t('tabs.failedTitle')}</h2>
        <p className="meta">
          <code>{address}</code>
        </p>
        <p>{errorText(t, failed?.error ?? 'unknown')}</p>
        <div className="row">
          <button
            type="button"
            className="primary"
            onClick={() => {
              reload(tab.key)
            }}
          >
            {t('transfers.retry')}
          </button>
          <button
            type="button"
            onClick={() => {
              close(tab.key)
            }}
          >
            {t('tabs.close')}
          </button>
        </div>
      </div>
    )
  }

  return (
    <div className="viewport-message" role="status">
      <Spinner />
      <h2>{tab.state === 'discarded' ? t('tabs.restoring') : t('tabs.loading')}</h2>
      <p className="meta">
        <code>{address}</code>
      </p>
      {percent !== null ? <progress max={100} value={percent} aria-label={address} /> : null}
      {tab.transferId !== null ? (
        <button
          type="button"
          onClick={() => {
            if (tab.transferId !== null) cancel(tab.transferId)
          }}
        >
          {t('transfers.cancel')}
        </button>
      ) : null}
    </div>
  )
}

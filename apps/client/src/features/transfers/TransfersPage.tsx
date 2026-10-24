import { formatBytes } from '../../shared/format.js'
import { shortAddress } from '../../shared/hue.js'
import { errorText, useT } from '../../shared/i18n/index.js'
import { Icon, Spinner } from '../../shared/ui/Icon.js'
import { isActive, useTransfers, type Transfer } from './model.js'

function Row({ transfer, locale }: { transfer: Transfer; locale: string }) {
  const t = useT()
  const { cancel, retry, dismiss } = useTransfers()
  const active = isActive(transfer)
  const percent = transfer.total > 0 ? Math.round((transfer.done / transfer.total) * 100) : 0

  return (
    <li className={`transfer transfer-${transfer.state}`}>
      <span className="transfer-icon" aria-hidden="true">
        {active ? <Spinner /> : null}
        {transfer.state === 'done' ? <Icon name="check" /> : null}
        {transfer.state === 'failed' ? <Icon name="alert" /> : null}
      </span>
      <div className="transfer-body">
        <div className="transfer-head">
          <strong>{shortAddress(transfer.address)}</strong>
          <span className="transfer-actions">
            {active ? (
              <button
                type="button"
                onClick={() => {
                  cancel(transfer.id)
                }}
              >
                {t('transfers.cancel')}
              </button>
            ) : null}
            {transfer.state === 'failed' ? (
              <button
                type="button"
                onClick={() => {
                  retry(transfer.id)
                }}
              >
                {t('transfers.retry')}
              </button>
            ) : null}
            {!active ? (
              <button
                type="button"
                className="icon-button"
                aria-label={t('transfers.dismiss')}
                title={t('transfers.dismiss')}
                onClick={() => {
                  dismiss(transfer.id)
                }}
              >
                <Icon name="x" />
              </button>
            ) : null}
          </span>
        </div>
        {transfer.state === 'connecting' ? <p className="meta">{t('transfers.waiting')}</p> : null}
        {transfer.state === 'downloading' ? (
          <>
            <progress max={transfer.total} value={transfer.done} aria-label={transfer.address} />
            <p className="meta">
              {t('transfers.progress', {
                done: formatBytes(transfer.done, locale),
                total: formatBytes(transfer.total, locale),
              })}{' '}
              ({percent}%)
            </p>
          </>
        ) : null}
        {transfer.state === 'done' ? <p className="meta">{t('transfers.done')}</p> : null}
        {transfer.state === 'done' && transfer.stale ? (
          <p role="note" className="notice notice-warn">
            {t('transfers.stale')}
          </p>
        ) : null}
        {transfer.state === 'failed' ? (
          <p role="alert" className="notice notice-error">
            {t('transfers.failed')}: {errorText(t, transfer.error ?? 'unknown')}
          </p>
        ) : null}
      </div>
    </li>
  )
}

export function TransfersPage({ locale }: { locale: string }) {
  const t = useT()
  const { transfers, clearFinished } = useTransfers()
  const finished = transfers.some((transfer) => !isActive(transfer))
  return (
    <section aria-labelledby="transfers-title">
      <div className="page-head page-head-row">
        <h1 id="transfers-title">{t('transfers.title')}</h1>
        {finished ? (
          <button type="button" onClick={clearFinished}>
            {t('transfers.clear')}
          </button>
        ) : null}
      </div>
      {transfers.length === 0 ? (
        <div className="empty">
          <span className="empty-art">
            <Icon name="download" />
          </span>
          <h3>{t('transfers.none')}</h3>
          <p>{t('transfers.noneHint')}</p>
        </div>
      ) : (
        <ul className="stack">
          {transfers.map((transfer) => (
            <Row key={transfer.id} transfer={transfer} locale={locale} />
          ))}
        </ul>
      )}
    </section>
  )
}

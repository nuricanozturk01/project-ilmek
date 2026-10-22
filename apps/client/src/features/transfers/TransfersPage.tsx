import { errorText, useT } from '../../shared/i18n/index.js'
import { formatBytes } from '../../shared/format.js'
import { useTransfers, type Transfer } from './model.js'

function Row({ transfer, locale }: { transfer: Transfer; locale: string }) {
  const t = useT()
  const { cancel } = useTransfers()
  const active = transfer.state === 'connecting' || transfer.state === 'downloading'
  const percent = transfer.total > 0 ? Math.round((transfer.done / transfer.total) * 100) : 0

  return (
    <li className="transfer">
      <div className="transfer-head">
        <strong>{transfer.address}</strong>
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
      </div>
      {transfer.state === 'connecting' ? <p>{t('transfers.waiting')}</p> : null}
      {transfer.state === 'downloading' ? (
        <>
          <progress max={transfer.total} value={transfer.done} aria-label={transfer.address} />
          <p>
            {t('transfers.progress', {
              done: formatBytes(transfer.done, locale),
              total: formatBytes(transfer.total, locale),
            })}{' '}
            ({percent}%)
          </p>
        </>
      ) : null}
      {transfer.state === 'done' ? <p>{t('transfers.done')}</p> : null}
      {transfer.state === 'done' && transfer.stale ? (
        <p role="note">{t('transfers.stale')}</p>
      ) : null}
      {transfer.state === 'failed' ? (
        <p role="alert">
          {t('transfers.failed')}: {errorText(t, transfer.error ?? 'unknown')}
        </p>
      ) : null}
    </li>
  )
}

export function TransfersPage({ locale }: { locale: string }) {
  const t = useT()
  const { transfers } = useTransfers()
  return (
    <section aria-labelledby="transfers-title">
      <h2 id="transfers-title">{t('transfers.title')}</h2>
      {transfers.length === 0 ? (
        <p>{t('transfers.none')}</p>
      ) : (
        <ul className="plain">
          {transfers.map((transfer) => (
            <Row key={transfer.id} transfer={transfer} locale={locale} />
          ))}
        </ul>
      )}
    </section>
  )
}

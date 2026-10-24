import { hueClass, shortAddress } from '../../shared/hue.js'
import { useT } from '../../shared/i18n/index.js'
import { Icon } from '../../shared/ui/Icon.js'
import { useTabs } from '../tabs/model.js'
import { Catalog } from './Catalog.js'
import type { HistoryApi } from './useHistory.js'

function Tile({ address, bookmarked }: { address: string; bookmarked: boolean }) {
  const { open } = useTabs()
  return (
    <li>
      <button
        type="button"
        className="tile"
        onClick={() => {
          open(address)
        }}
      >
        <span className={`avatar avatar-sm ${hueClass(address)}`} aria-hidden="true">
          {bookmarked ? <Icon name="star" filled /> : <Icon name="compass" />}
        </span>
        <span className="tile-text">{shortAddress(address)}</span>
      </button>
    </li>
  )
}

function Quick({
  title,
  addresses,
  bookmarks,
}: {
  title: string
  addresses: string[]
  bookmarks: boolean
}) {
  const t = useT()
  return (
    <section className="quick-group">
      <h3>{title}</h3>
      {addresses.length === 0 ? (
        <p className="meta">{t('history.none')}</p>
      ) : (
        <ul className="tiles">
          {addresses.slice(0, 8).map((address) => (
            <Tile key={address} address={address} bookmarked={bookmarks} />
          ))}
        </ul>
      )}
    </section>
  )
}

export function DiscoverPage({ history }: { history: HistoryApi }) {
  const t = useT()
  return (
    <>
      <div className="page-head">
        <h1>{t('discover.title')}</h1>
        <p>{t('discover.subtitle')}</p>
      </div>
      <section className="quick" aria-label={t('discover.quick')}>
        <Quick title={t('history.bookmarks')} addresses={history.history.bookmarks} bookmarks />
        <Quick title={t('history.recent')} addresses={history.history.visited} bookmarks={false} />
      </section>
      <Catalog />
    </>
  )
}

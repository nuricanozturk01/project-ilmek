import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>()
const listeners = new Map<string, (event: { payload: unknown }) => void>()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (command: string, args?: Record<string, unknown>) => invoke(command, args),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler)
    return Promise.resolve(() => listeners.delete(name))
  },
}))

const { App } = await import('../src/app/App.js')
const { I18nProvider, createTranslate, errorText, pickLocale } =
  await import('../src/shared/i18n/index.js')
const { formatBytes } = await import('../src/shared/format.js')

function failure(code: string): Error {
  return Object.assign(new Error(code), { code })
}

const status = {
  community_id: '01a1647f-de10-755f-beb4-79875d9e6368',
  peer_id: '01a1647f-de10-7e1a-8acf-c35e224c957c',
  registry: '10.77.10.1:47401',
  registry_reachable: true,
}

const entry = {
  address: 'ilmek://nuri/forum',
  name: 'Forum',
  description: 'Ortak forum',
  publisher_id: 'p',
  version: '1.0.0',
  availability: 'recent_heartbeat' as const,
  cached: false,
}

function respond(overrides: Record<string, unknown> = {}) {
  invoke.mockImplementation((command) => {
    if (command in overrides) {
      const value = overrides[command]
      return typeof value === 'function'
        ? Promise.resolve((value as () => unknown)())
        : Promise.resolve(value)
    }
    const defaults: Record<string, unknown> = {
      shell_state: { joined: true, status },
      shell_catalog: { entries: [entry], next_cursor: null },
      shell_history: { visited: ['ilmek://nuri/merhaba'], bookmarks: [] },
      shell_cached: [],
      shell_open: { label: 'app-1', stale: false },
      shell_cancel: null,
      shell_bookmark: null,
      shell_join: null,
      shell_refresh: status,
      shell_diagnostics: '{"ok":true}',
      shell_remove: null,
    }
    return Promise.resolve(defaults[command] ?? null)
  })
}

async function openFromCatalog(user: ReturnType<typeof userEvent.setup>) {
  const card = (await screen.findByText('Ortak forum')).closest('li') as HTMLElement
  await user.click(within(card).getByRole('button', { name: 'Aç' }))
}

function renderApp(locale: 'tr' | 'en' = 'tr') {
  return render(
    <I18nProvider locale={locale}>
      <App locale={locale} />
    </I18nProvider>,
  )
}

beforeEach(() => {
  invoke.mockReset()
  listeners.clear()
})

describe('joining a community', () => {
  it('sends the profile and never persists it in the page', async () => {
    respond({ shell_state: { joined: false, status: null } })
    const user = userEvent.setup()
    renderApp()

    await user.type(await screen.findByLabelText('Topluluk profili (JSON)'), '{{"a":1}')
    await user.click(screen.getByRole('button', { name: 'Bul ve katıl' }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('shell_join', {
        profile: '{"a":1}',
        registry: null,
      })
    })
    expect(window.localStorage.length).toBe(0)
  })

  it('finds the node by itself when nothing is typed', async () => {
    respond({ shell_state: { joined: false, status: null } })
    const user = userEvent.setup()
    renderApp()

    await user.click(await screen.findByRole('button', { name: 'Bul ve katıl' }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('shell_join', { profile: null, registry: null })
    })
  })

  it('says what to do when no node answers', async () => {
    invoke.mockImplementation((command) =>
      command === 'shell_state'
        ? Promise.resolve({ joined: false, status: null })
        : Promise.reject(failure('no_node')),
    )
    const user = userEvent.setup()
    renderApp()
    await user.click(await screen.findByRole('button', { name: 'Bul ve katıl' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('düğümü bulunamadı')
  })

  it('explains a rejected profile in plain language', async () => {
    respond({ shell_state: { joined: false, status: null } })
    invoke.mockImplementation((command) =>
      command === 'shell_state'
        ? Promise.resolve({ joined: false, status: null })
        : Promise.reject(failure('profile_invalid')),
    )
    const user = userEvent.setup()
    renderApp()
    await user.type(await screen.findByLabelText('Topluluk profili (JSON)'), '{{}')
    await user.click(screen.getByRole('button', { name: 'Bul ve katıl' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Profil geçersiz')
  })
})

describe('discovering and opening applications', () => {
  it('lists the catalog and opens an application through the shell', async () => {
    respond()
    const user = userEvent.setup()
    renderApp()

    const card = (await screen.findByText('Ortak forum')).closest('li')
    expect(card).not.toBeNull()
    expect(within(card as HTMLElement).getByText(/Sunucu etkin/)).toBeInTheDocument()
    await user.click(within(card as HTMLElement).getByRole('button', { name: 'Aç' }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'shell_open',
        expect.objectContaining({ address: 'ilmek://nuri/forum' }),
      )
    })
  })

  it('opens a typed address and offers a bookmark', async () => {
    respond()
    const user = userEvent.setup()
    renderApp()
    const input = await screen.findByLabelText('Uygulama adresi')
    await user.type(input, 'ilmek://nuri/merhaba')
    await user.click(screen.getByRole('button', { name: 'Yer imi ekle' }))
    expect(invoke).toHaveBeenCalledWith('shell_bookmark', {
      address: 'ilmek://nuri/merhaba',
      on: true,
    })
    await user.click(
      within(input.closest('form') as HTMLElement).getByRole('button', { name: 'Aç' }),
    )
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'shell_open',
        expect.objectContaining({ address: 'ilmek://nuri/merhaba' }),
      )
    })
  })

  it('shows the real byte progress of a transfer and lets the user cancel it', async () => {
    let finish: (value: unknown) => void = () => undefined
    respond()
    invoke.mockImplementation((command) => {
      if (command === 'shell_open')
        return new Promise((resolve) => {
          finish = resolve
        })
      if (command === 'shell_state') return Promise.resolve({ joined: true, status })
      if (command === 'shell_catalog')
        return Promise.resolve({ entries: [entry], next_cursor: null })
      if (command === 'shell_history') return Promise.resolve({ visited: [], bookmarks: [] })
      return Promise.resolve(null)
    })
    const user = userEvent.setup()
    renderApp()
    await openFromCatalog(user)
    await user.click(screen.getByRole('button', { name: 'Aktarımlar' }))
    expect(await screen.findByText('Bağlanıyor…')).toBeInTheDocument()

    const id = (invoke.mock.calls.find(([c]) => c === 'shell_open')?.[1] as { transferId: string })
      .transferId
    listeners.get('transfer-progress')?.({
      payload: { id, bytes_done: 1_572_864, bytes_total: 3_145_728 },
    })
    expect(await screen.findByText(/1,5 MiB \/ 3 MiB/)).toBeInTheDocument()
    expect(screen.getByRole('progressbar')).toHaveAttribute('value', '1572864')

    await user.click(screen.getByRole('button', { name: 'İptal' }))
    expect(invoke).toHaveBeenCalledWith('shell_cancel', { transferId: id })
    finish({ label: 'app-1', stale: false })
  })

  it('reports a failed transfer with a translated reason and warns about stale names', async () => {
    respond()
    invoke.mockImplementation((command) => {
      if (command === 'shell_open') return Promise.reject(failure('integrity'))
      if (command === 'shell_state') return Promise.resolve({ joined: true, status })
      if (command === 'shell_catalog')
        return Promise.resolve({ entries: [entry], next_cursor: null })
      if (command === 'shell_history') return Promise.resolve({ visited: [], bookmarks: [] })
      return Promise.resolve(null)
    })
    const user = userEvent.setup()
    renderApp()
    await openFromCatalog(user)
    await user.click(screen.getByRole('button', { name: 'Aktarımlar' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('doğrulanamadı; açılmadı')
  })
})

describe('settings', () => {
  it('shows status and lists cached apps', async () => {
    respond({
      shell_state: {
        joined: true,
        status,
      },
      shell_cached: [{ address: 'app-id', version: '1.0.0-abc', bytes: 2048 }],
    })
    const user = userEvent.setup()
    renderApp()
    await user.click(await screen.findByRole('button', { name: 'Ayarlar' }))

    expect(await screen.findByText(/10\.77\.10\.1:47401/)).toBeInTheDocument()
    expect(await screen.findByText(/2 KiB/)).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Tanı bilgisini göster' }))
    expect(await screen.findByText('{"ok":true}')).toBeInTheDocument()
    expect(screen.getByText('Uygulama içeriği içermez.')).toBeInTheDocument()
  })

  it('is fully available in English', async () => {
    respond()
    renderApp('en')
    expect(await screen.findByRole('button', { name: 'Settings' })).toBeInTheDocument()
  })
})

describe('localisation helpers', () => {
  it('picks a supported locale and falls back to Turkish', () => {
    expect(pickLocale(['en-US', 'tr'])).toBe('en')
    expect(pickLocale(['de-DE', 'tr-TR'])).toBe('tr')
    expect(pickLocale(['fr'])).toBe('tr')
  })

  it('translates with parameters and never throws on an unknown error code', () => {
    const t = createTranslate('en')
    expect(t('catalog.version', { version: '2.0.0' })).toBe('Version 2.0.0')
    expect(errorText(t, 'no-such-code')).toBe('An unexpected error occurred.')
    expect(errorText(createTranslate('tr'), 'quota')).toBe('Önbellekte yeterli yer yok.')
  })

  it('formats sizes with the locale', () => {
    expect(formatBytes(1536, 'en')).toBe('1.5 KiB')
    expect(formatBytes(1536, 'tr')).toBe('1,5 KiB')
    expect(formatBytes(12, 'en')).toBe('12 B')
  })
})

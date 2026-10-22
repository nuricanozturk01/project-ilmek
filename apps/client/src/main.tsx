import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './app/App.js'
import { I18nProvider, pickLocale } from './shared/i18n/index.js'
import './styles.css'

const locale = pickLocale(navigator.languages)
document.documentElement.lang = locale

const container = document.getElementById('root')
if (container === null) throw new Error('missing #root')
createRoot(container).render(
  <StrictMode>
    <I18nProvider locale={locale}>
      <App locale={locale} />
    </I18nProvider>
  </StrictMode>,
)

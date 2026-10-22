import { createContext, useContext, useMemo, type ReactNode } from 'react'
import { en } from './en.js'
import { tr, type Messages } from './tr.js'

type Path<T, P extends string = ''> = {
  [K in keyof T & string]: T[K] extends string
    ? `${P}${K}`
    : T[K] extends Record<string, unknown>
      ? Path<T[K], `${P}${K}.`>
      : never
}[keyof T & string]

export type MessageKey = Path<Messages>
export type Translate = (key: MessageKey, values?: Record<string, string | number>) => string

const catalogs: Record<string, Messages> = { tr, en }

export function pickLocale(languages: readonly string[]): 'tr' | 'en' {
  for (const language of languages) {
    const base = language.toLowerCase().split('-')[0]
    if (base === 'tr' || base === 'en') return base
  }
  return 'tr'
}

function lookup(messages: Messages, key: string): string {
  let current: unknown = messages
  for (const part of key.split('.')) {
    if (typeof current !== 'object' || current === null) return key
    current = (current as Record<string, unknown>)[part]
  }
  return typeof current === 'string' ? current : key
}

export function createTranslate(locale: 'tr' | 'en'): Translate {
  const messages = catalogs[locale] ?? tr
  return (key, values) =>
    lookup(messages, key).replace(/\{(\w+)\}/g, (_, name: string) => String(values?.[name] ?? ''))
}

const TranslateContext = createContext<Translate>(createTranslate('tr'))

export function I18nProvider({ locale, children }: { locale: 'tr' | 'en'; children: ReactNode }) {
  const translate = useMemo(() => createTranslate(locale), [locale])
  return <TranslateContext value={translate}>{children}</TranslateContext>
}

export function useT(): Translate {
  return useContext(TranslateContext)
}

/** Translates a shell error code, falling back to the generic message. */
export function errorText(translate: Translate, code: string): string {
  const key = `errors.${code}` as MessageKey
  const text = translate(key)
  return text === key ? translate('errors.unknown') : text
}

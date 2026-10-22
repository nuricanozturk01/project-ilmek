const UNITS = ['B', 'KiB', 'MiB', 'GiB'] as const

export function formatBytes(bytes: number, locale: string): string {
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024
    unit += 1
  }
  const number = new Intl.NumberFormat(locale, { maximumFractionDigits: unit === 0 ? 0 : 1 })
  return `${number.format(value)} ${UNITS[unit] ?? 'B'}`
}

export function formatDate(epochMs: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(
    new Date(epochMs),
  )
}

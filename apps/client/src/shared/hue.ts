/** Stable colour slot (0-7) for a name, so the same app always gets the same avatar colour. */
export function hueOf(text: string): number {
  let sum = 0
  for (const char of text) sum = (sum * 31 + (char.codePointAt(0) ?? 0)) % 997
  return sum % 8
}

/** Up to two letters for an avatar. */
export function initials(text: string): string {
  const words = text.trim().split(/[\s/_-]+/).filter(Boolean)
  const letters = words.slice(0, 2).map((word) => word.slice(0, 1))
  return (letters.join('') || '?').toLocaleUpperCase()
}

/** `ilmek://nuri/forum` from what a person typed (`nuri/forum` is enough). */
export function normalizeAddress(text: string): string {
  const trimmed = text.trim()
  if (trimmed === '' || trimmed.includes('://')) return trimmed
  return `ilmek://${trimmed.replace(/^\/+/, '')}`
}

/** `nuri/forum` for display. */
export function shortAddress(address: string): string {
  return address.replace(/^ilmek:\/\//, '')
}

/** CSS class carrying the colour slot of `text`. */
export function hueClass(text: string): string {
  return `hue-${String(hueOf(text))}`
}

/** Random source; replaceable in tests. */
export type RandomBytes = (length: number) => Uint8Array

const secureRandom: RandomBytes = (length) => {
  const bytes = new Uint8Array(length)
  globalThis.crypto.getRandomValues(bytes)
  return bytes
}

/**
 * RFC 9562 UUIDv7. `crypto.randomUUID()` produces version 4, which İlmek rejects, and relabelling
 * a v4 would be wrong, so identifiers are built from a timestamp and random bits here.
 */
export function uuidv7(now: number = Date.now(), random: RandomBytes = secureRandom): string {
  const bytes = new Uint8Array(16)
  let remaining = Math.floor(now)
  for (let index = 5; index >= 0; index -= 1) {
    bytes[index] = remaining % 256
    remaining = Math.floor(remaining / 256)
  }
  bytes.set(random(10), 6)
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x70
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

const UUID7 = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/

export function isUuidv7(text: string): boolean {
  return UUID7.test(text)
}

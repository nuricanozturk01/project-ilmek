import { describe, expect, it } from 'vitest'
import { isUuidv7, uuidv7 } from '../src/uuid7.js'

describe('uuidv7', () => {
  it('produces canonical lowercase version-7 identifiers', () => {
    for (let n = 0; n < 200; n += 1) {
      const id = uuidv7()
      expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
      expect(isUuidv7(id)).toBe(true)
    }
  })

  it('encodes the millisecond timestamp in the first 48 bits', () => {
    const at = 0x01a1_647f_de10
    const id = uuidv7(at, () => new Uint8Array(10))
    expect(id.replaceAll('-', '').slice(0, 12)).toBe('01a1647fde10')
  })

  it('is sortable by creation time and uses the supplied randomness', () => {
    const early = uuidv7(1_000, () => new Uint8Array(10).fill(0xff))
    const late = uuidv7(2_000, () => new Uint8Array(10))
    expect(early < late).toBe(true)
    expect(early.endsWith('ffffffffffff')).toBe(true)
  })

  it('never accepts other versions, uppercase or malformed text', () => {
    expect(isUuidv7('f47ac10b-58cc-4372-a567-0e02b2c3d479')).toBe(false)
    expect(isUuidv7('01A1647F-DE10-755F-BEB4-79875D9E6368')).toBe(false)
    expect(isUuidv7('00000000-0000-0000-0000-000000000000')).toBe(false)
    expect(isUuidv7('')).toBe(false)
  })
})

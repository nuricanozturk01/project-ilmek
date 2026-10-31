import '@angular/compiler'
import {
  HttpErrorResponse,
  HttpHeaders,
  HttpParams,
  HttpRequest,
  HttpResponse,
} from '@angular/common/http'
import type { HttpEvent, HttpHandlerFn } from '@angular/common/http'
import { firstValueFrom, of } from 'rxjs'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ilmekHttpInterceptor, provideIlmekHttp } from '../src/index'

interface FakeWindow {
  ilmek?: { call: ReturnType<typeof vi.fn> }
  location: { origin: string }
}

const globals = globalThis as unknown as { window?: FakeWindow }
let call: ReturnType<typeof vi.fn>
const passthrough = new HttpResponse({ status: 200, body: 'next' })
let next: ReturnType<typeof vi.fn<HttpHandlerFn>>

function run(
  req: HttpRequest<unknown>,
  options?: { prefix?: string },
): Promise<HttpEvent<unknown>> {
  return firstValueFrom(ilmekHttpInterceptor(options)(req, next))
}

async function failure(req: HttpRequest<unknown>): Promise<HttpErrorResponse> {
  try {
    await run(req)
  } catch (e) {
    return e as HttpErrorResponse
  }
  throw new Error('expected an error')
}

beforeEach(() => {
  call = vi.fn().mockResolvedValue({ status: 200, body: { ok: true } })
  globals.window = { ilmek: { call }, location: { origin: 'http://localhost:4200' } }
  next = vi.fn<HttpHandlerFn>(() => of(passthrough))
})

afterEach(() => {
  delete globals.window
})

describe('reads', () => {
  it('sends GET to http.read without the write option, mapping query and headers', async () => {
    const params = new HttpParams().append('a', '1').append('a', '2').set('b', 'x')
    const headers = new HttpHeaders({ 'X-Trace': 't1', Accept: 'application/json' })
    const res = (await run(
      new HttpRequest('GET', '/api/tasks/1', { params, headers }),
    )) as HttpResponse<unknown>
    expect(call).toHaveBeenCalledTimes(1)
    expect(call.mock.calls[0]).toHaveLength(2)
    expect(call.mock.calls[0]?.[0]).toBe('http.read')
    expect(call.mock.calls[0]?.[1]).toEqual({
      method: 'GET',
      path: '/api/tasks/1',
      query: { a: ['1', '2'], b: ['x'] },
      headers: { 'x-trace': 't1', accept: 'application/json' },
    })
    expect(res.status).toBe(200)
    expect(res.body).toEqual({ ok: true })
    expect(next).not.toHaveBeenCalled()
  })

  it('omits empty query and headers and keeps the query out of the path', async () => {
    await run(new HttpRequest('GET', '/api/tasks'))
    expect(call.mock.calls[0]?.[1]).toEqual({ method: 'GET', path: '/api/tasks' })
    await run(new HttpRequest('GET', '/api/tasks?done=1'))
    expect(call.mock.calls[1]?.[1]).toEqual({
      method: 'GET',
      path: '/api/tasks',
      query: { done: ['1'] },
    })
  })

  it.each(['HEAD', 'OPTIONS'] as const)('routes %s to http.read', async (method) => {
    await run(
      method === 'HEAD' ? new HttpRequest('HEAD', '/api/x') : new HttpRequest('OPTIONS', '/api/x'),
    )
    expect(call.mock.calls[0]?.[0]).toBe('http.read')
    expect((call.mock.calls[0]?.[1] as { method: string }).method).toBe(method)
  })

  it('accepts same-origin absolute URLs and rejects other origins as pass-through', async () => {
    await run(new HttpRequest('GET', 'http://localhost:4200/api/a'))
    expect(call.mock.calls[0]?.[1]).toMatchObject({ path: '/api/a' })
    const res = await run(new HttpRequest('GET', 'https://example.com/api/a'))
    expect(res).toBe(passthrough)
    expect(call).toHaveBeenCalledTimes(1)
  })
})

describe('writes', () => {
  it.each(['POST', 'PUT', 'PATCH', 'DELETE'] as const)(
    'routes %s to http.write with write option',
    async (method) => {
      const req =
        method === 'DELETE'
          ? new HttpRequest(method, '/api/tasks/1')
          : new HttpRequest(method, '/api/tasks/1', { title: 'a' })
      await run(req)
      expect(call.mock.calls[0]?.[0]).toBe('http.write')
      expect(call.mock.calls[0]?.[2]).toEqual({ write: true })
      const envelope = call.mock.calls[0]?.[1] as { method: string; body?: unknown }
      expect(envelope.method).toBe(method)
      if (method === 'DELETE') expect('body' in envelope).toBe(false)
      else expect(envelope.body).toEqual({ title: 'a' })
    },
  )
})

describe('pass-through and prefix', () => {
  it('passes non-matching URLs to next', async () => {
    const req = new HttpRequest('GET', '/assets/x.json')
    expect(await run(req)).toBe(passthrough)
    expect(next).toHaveBeenCalledWith(req)
    expect(call).not.toHaveBeenCalled()
  })

  it('does not match a prefix that is only a string prefix of a segment', async () => {
    await run(new HttpRequest('GET', '/apix/a'))
    expect(call).not.toHaveBeenCalled()
  })

  it('honours a custom prefix', async () => {
    await run(new HttpRequest('GET', '/v2/items'), { prefix: '/v2' })
    expect(call.mock.calls[0]?.[1]).toMatchObject({ path: '/v2/items' })
    await run(new HttpRequest('GET', '/api/items'), { prefix: '/v2' })
    expect(next).toHaveBeenCalledTimes(1)
  })
})

describe('responses', () => {
  it('maps 201 to HttpResponse with headers', async () => {
    call.mockResolvedValue({ status: 201, headers: { location: '/api/tasks/2' }, body: { id: 2 } })
    const res = (await run(new HttpRequest('POST', '/api/tasks', {}))) as HttpResponse<unknown>
    expect(res).toBeInstanceOf(HttpResponse)
    expect(res.status).toBe(201)
    expect(res.headers.get('location')).toBe('/api/tasks/2')
    expect(res.body).toEqual({ id: 2 })
  })

  it('maps 404 to HttpErrorResponse with the body as error', async () => {
    call.mockResolvedValue({ status: 404, body: { error: 'missing' } })
    const err = await failure(new HttpRequest('GET', '/api/tasks/9'))
    expect(err).toBeInstanceOf(HttpErrorResponse)
    expect(err.status).toBe(404)
    expect(err.error).toEqual({ error: 'missing' })
    expect(err.url).toBe('/api/tasks/9')
  })

  it('uses text when there is no body, and null when neither is present', async () => {
    call.mockResolvedValue({ status: 200, text: 'hello' })
    expect(((await run(new HttpRequest('GET', '/api/t'))) as HttpResponse<unknown>).body).toBe(
      'hello',
    )
    call.mockResolvedValue({ status: 204 })
    expect(((await run(new HttpRequest('GET', '/api/t'))) as HttpResponse<unknown>).body).toBeNull()
  })

  it('returns a string for responseType text, serializing JSON bodies', async () => {
    call.mockResolvedValue({ status: 200, body: { a: 1 } })
    const res = (await run(
      new HttpRequest('GET', '/api/t', { responseType: 'text' }),
    )) as HttpResponse<unknown>
    expect(res.body).toBe('{"a":1}')
    call.mockResolvedValue({ status: 200, text: 'plain' })
    const res2 = (await run(
      new HttpRequest('GET', '/api/t', { responseType: 'text' }),
    )) as HttpResponse<unknown>
    expect(res2.body).toBe('plain')
  })
})

describe('request headers', () => {
  it('drops forbidden headers and lowercases the rest', async () => {
    const headers = new HttpHeaders({
      Cookie: 'a=b',
      Authorization: 'Bearer x',
      Host: 'h',
      'Content-Length': '3',
      Origin: 'o',
      Referer: 'r',
      'X-Keep': 'k',
    })
    await run(new HttpRequest('GET', '/api/t', { headers }))
    expect(call.mock.calls[0]?.[1]).toEqual({
      method: 'GET',
      path: '/api/t',
      headers: { 'x-keep': 'k' },
    })
  })
})

describe('rejections', () => {
  it('rejects FormData bodies', async () => {
    const err = await failure(new HttpRequest('POST', '/api/up', new FormData()))
    expect(err).toBeInstanceOf(HttpErrorResponse)
    expect(err.status).toBe(0)
    expect(err.message.startsWith('ilmek:')).toBe(true)
    expect(call).not.toHaveBeenCalled()
    expect(next).not.toHaveBeenCalled()
  })

  it.each([
    ['Blob', () => new Blob(['x'])],
    ['ArrayBuffer', () => new ArrayBuffer(2)],
    ['Uint8Array', () => new Uint8Array(2)],
  ])('rejects %s bodies', async (_n, make) => {
    const err = await failure(new HttpRequest('PUT', '/api/up', make()))
    expect(err.status).toBe(0)
    expect(err.message.startsWith('ilmek:')).toBe(true)
    expect(call).not.toHaveBeenCalled()
  })

  it.each(['blob', 'arraybuffer'] as const)('rejects responseType %s', async (responseType) => {
    const err = await failure(new HttpRequest('GET', '/api/f', { responseType }))
    expect(err.status).toBe(0)
    expect(err.message.startsWith('ilmek:')).toBe(true)
    expect(call).not.toHaveBeenCalled()
  })

  it('fails with an ilmek: error when window.ilmek is missing, without touching the network', async () => {
    delete globals.window?.ilmek
    const err = await failure(new HttpRequest('GET', '/api/t'))
    expect(err.status).toBe(0)
    expect(err.message.startsWith('ilmek:')).toBe(true)
    expect(next).not.toHaveBeenCalled()
  })

  it('fails the same way when there is no window at all', async () => {
    delete globals.window
    const err = await failure(new HttpRequest('GET', '/api/t'))
    expect(err.status).toBe(0)
    expect(err.message.startsWith('ilmek:')).toBe(true)
  })

  it('turns a rejected call into a status 0 error carrying the original message', async () => {
    call.mockRejectedValue(new Error('connection lost'))
    const err = await failure(new HttpRequest('POST', '/api/t', {}))
    expect(err).toBeInstanceOf(HttpErrorResponse)
    expect(err.status).toBe(0)
    expect(err.error).toBe('connection lost')
  })
})

describe('provideIlmekHttp', () => {
  it('returns environment providers', () => {
    expect(provideIlmekHttp()).toBeDefined()
    expect(provideIlmekHttp({ prefix: '/x' })).toBeDefined()
  })
})

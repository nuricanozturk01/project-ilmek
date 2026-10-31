import {
  HttpErrorResponse,
  HttpHeaders,
  HttpResponse,
  provideHttpClient,
  withInterceptors,
} from '@angular/common/http'
import type { HttpEvent, HttpHandlerFn, HttpInterceptorFn, HttpRequest } from '@angular/common/http'
import type { EnvironmentProviders } from '@angular/core'
import { catchError, defer, from, of, switchMap, throwError } from 'rxjs'
import type { Observable } from 'rxjs'
import type { IlmekCaller } from './global'

export type { IlmekCaller, IlmekCallOptions } from './global'

export interface IlmekHttpOptions {
  prefix?: string
}

interface HostResponse {
  status: number
  headers?: Record<string, string>
  body?: unknown
  text?: string
}

interface Envelope {
  method: string
  path: string
  query?: Record<string, string[]>
  headers?: Record<string, string>
  body?: unknown
}

const DEFAULT_PREFIX = '/api'
const READ_METHODS = new Set(['GET', 'HEAD', 'OPTIONS'])
const DROPPED_HEADERS = new Set([
  'cookie',
  'authorization',
  'host',
  'content-length',
  'origin',
  'referer',
])
const PLACEHOLDER_ORIGIN = 'http://ilmek.invalid'

function localFailure(
  req: HttpRequest<unknown>,
  message: string,
  statusText = 'Unknown Error',
): HttpErrorResponse {
  const err = new HttpErrorResponse({ status: 0, statusText, url: req.url, error: message })
  Object.defineProperty(err, 'message', { value: message })
  return err
}

function pathOf(url: string): URL | null {
  const absolute = /^[a-z][a-z0-9+.-]*:\/\//i.test(url) || url.startsWith('//')
  if (absolute) {
    const here =
      typeof window === 'undefined' ? undefined : (window.location as Location | undefined)?.origin
    const parsed = new URL(url, here ?? PLACEHOLDER_ORIGIN)
    return parsed.origin === here ? parsed : null
  }
  return new URL(url, PLACEHOLDER_ORIGIN)
}

function matches(path: string, prefix: string): boolean {
  return path === prefix || path.startsWith(prefix.endsWith('/') ? prefix : prefix + '/')
}

function isBinaryBody(body: unknown): boolean {
  return (
    (typeof FormData !== 'undefined' && body instanceof FormData) ||
    (typeof Blob !== 'undefined' && body instanceof Blob) ||
    body instanceof ArrayBuffer ||
    ArrayBuffer.isView(body)
  )
}

function buildEnvelope(req: HttpRequest<unknown>, url: URL, write: boolean): Envelope {
  const query: Record<string, string[]> = {}
  const add = (name: string, value: string): void => {
    ;(query[name] ??= []).push(value)
  }
  url.searchParams.forEach((value, name) => {
    add(name, value)
  })
  for (const name of req.params.keys()) {
    for (const value of req.params.getAll(name) ?? []) add(name, value)
  }
  const headers: Record<string, string> = {}
  for (const name of req.headers.keys()) {
    const lower = name.toLowerCase()
    if (DROPPED_HEADERS.has(lower)) continue
    headers[lower] = (req.headers.getAll(name) ?? []).join(', ')
  }
  const envelope: Envelope = { method: req.method.toUpperCase(), path: url.pathname }
  if (Object.keys(query).length > 0) envelope.query = query
  if (Object.keys(headers).length > 0) envelope.headers = headers
  if (write && req.body !== null && req.body !== undefined) envelope.body = req.body
  return envelope
}

function toEvent(req: HttpRequest<unknown>, res: HostResponse): Observable<HttpEvent<unknown>> {
  const headers = new HttpHeaders(res.headers ?? {})
  const raw = res.body ?? res.text ?? null
  if (res.status >= 400) {
    return throwError(
      () =>
        new HttpErrorResponse({
          status: res.status,
          statusText: String(res.status),
          headers,
          url: req.url,
          error: raw,
        }),
    )
  }
  let body: unknown = raw
  if (req.responseType === 'text' && raw !== null && typeof raw !== 'string')
    body = JSON.stringify(raw)
  return of(
    new HttpResponse({
      status: res.status,
      statusText: String(res.status),
      headers,
      url: req.url,
      body,
    }),
  )
}

function fail(req: HttpRequest<unknown>, message: string): Observable<never> {
  return throwError(() => localFailure(req, message))
}

function callerOf(): IlmekCaller | undefined {
  return typeof window === 'undefined' ? undefined : window.ilmek
}

function failureMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

export function ilmekHttpInterceptor(options?: IlmekHttpOptions): HttpInterceptorFn {
  const prefix = options?.prefix ?? DEFAULT_PREFIX
  return (req: HttpRequest<unknown>, next: HttpHandlerFn): Observable<HttpEvent<unknown>> => {
    const url = pathOf(req.url)
    if (url === null || !matches(url.pathname, prefix)) return next(req)

    return defer(() => {
      const caller = callerOf()
      if (caller === undefined) {
        return fail(
          req,
          'ilmek: window.ilmek is missing; the app is not running inside the Ilmek client',
        )
      }
      if (req.responseType === 'blob' || req.responseType === 'arraybuffer') {
        return fail(req, `ilmek: responseType '${req.responseType}' is not supported`)
      }
      if (isBinaryBody(req.body)) {
        return fail(req, 'ilmek: FormData, Blob and binary request bodies are not supported')
      }
      const write = !READ_METHODS.has(req.method.toUpperCase())
      const envelope = buildEnvelope(req, url, write)
      const pending = write
        ? caller.call<HostResponse>('http.write', envelope, { write: true })
        : caller.call<HostResponse>('http.read', envelope)
      return from(pending).pipe(
        catchError((e: unknown) => fail(req, failureMessage(e))),
        switchMap((res) => toEvent(req, res)),
      )
    })
  }
}

export function provideIlmekHttp(options?: IlmekHttpOptions): EnvironmentProviders {
  return provideHttpClient(withInterceptors([ilmekHttpInterceptor(options)]))
}

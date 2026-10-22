'use strict'
// A deliberately hostile page. Every attempt below must fail; the outcome of each is stored through
// the one channel the page legitimately has (SDK storage) so a test driver can read it from outside.
// Network attempts all target the driver's canary listener, which must record zero requests.

const CANARY = 'http://127.0.0.1:18080'
const results = {}
const sdk = window.ilmek

async function attempt(name, action) {
  try {
    results[name] = await action()
  } catch (error) {
    results[name] = `blocked:${error?.name ?? 'error'}`
  }
}

const later = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function main() {
  await attempt('fetch_external', async () => { await fetch(`${CANARY}/fetch`); return 'ALLOWED' })
  await attempt('fetch_no_cors', async () => { await fetch(`${CANARY}/nocors`, { mode: 'no-cors' }); return 'ALLOWED' })
  await attempt('xhr', () => new Promise((resolve, reject) => {
    const request = new XMLHttpRequest()
    request.open('GET', `${CANARY}/xhr`)
    request.onload = () => { resolve('ALLOWED') }
    request.onerror = () => { reject(new Error('blocked')) }
    request.send()
  }))
  await attempt('websocket', () => new Promise((resolve, reject) => {
    const socket = new WebSocket(`ws://127.0.0.1:18080/ws`)
    socket.onopen = () => { resolve('ALLOWED') }
    socket.onerror = () => { reject(new Error('blocked')) }
  }))
  await attempt('beacon', async () => (navigator.sendBeacon(`${CANARY}/beacon`, 'x') ? 'QUEUED' : 'blocked:refused'))
  await attempt('file_url', async () => { await fetch('file:///etc/hostname'); return 'ALLOWED' })
  await attempt('other_app_origin', async () => {
    await fetch('ilmekapp://app-01a1647f-de10-755f-beb4-79875d9e6368/index.html')
    return 'ALLOWED'
  })
  await attempt('eval', async () => String(eval('1 + 1')))
  await attempt('function_constructor', async () => String(new Function('return 2')()))
  await attempt('inline_script', async () => {
    const script = document.createElement('script')
    script.textContent = 'window.__pwned = true'
    document.head.append(script)
    await later(200)
    return window.__pwned === true ? 'ALLOWED' : 'blocked:inert'
  })
  await attempt('external_script', async () => {
    const script = document.createElement('script')
    script.src = `${CANARY}/script.js`
    document.head.append(script)
    await later(300)
    return 'issued'
  })
  await attempt('iframe', async () => {
    const frame = document.createElement('iframe')
    frame.src = `${CANARY}/frame`
    document.body.append(frame)
    await later(300)
    return 'issued'
  })
  await attempt('image', async () => {
    const image = new Image()
    image.src = `${CANARY}/image.png`
    await later(300)
    return 'issued'
  })
  await attempt('form_post', async () => {
    const form = document.createElement('form')
    form.action = `${CANARY}/form`
    form.method = 'post'
    document.body.append(form)
    form.submit()
    await later(300)
    return location.origin.startsWith('ilmekapp') || location.protocol === 'http:' ? 'stayed' : 'moved'
  })
  const startingLocation = location.href
  await attempt('popup', async () => (window.open(`${CANARY}/popup`) === null ? 'blocked:null' : 'ALLOWED'))
  await attempt('navigation', async () => {
    location.assign(`${CANARY}/nav`)
    await later(500)
    return location.href === startingLocation || location.href.startsWith(startingLocation.split('/').slice(0, 3).join('/')) ? 'stayed' : 'MOVED'
  })

  const internals = window.__TAURI_INTERNALS__
  await attempt('shell_state', async () => { await internals.invoke('shell_state'); return 'ALLOWED' })
  await attempt('shell_open', async () => { await internals.invoke('shell_open', { address: 'ilmek://x/y', transferId: 't' }); return 'ALLOWED' })
  await attempt('shell_join', async () => { await internals.invoke('shell_join', { profile: '{}', fingerprint: 'a', token: 'b', name: 'c' }); return 'ALLOWED' })
  await attempt('event_listen', async () => { await internals.invoke('plugin:event|listen', { event: 'transfer-progress', target: { kind: 'Any' }, handler: 0 }); return 'ALLOWED' })
  await attempt('window_create', async () => { await internals.invoke('plugin:webview|create_webview_window', { options: { label: 'evil', url: 'https://example.com' } }); return 'ALLOWED' })
  await attempt('bridge_foreign_field', async () => {
    const answer = await internals.invoke('ilmek_bridge', { request: { v: 1, op: 'storage.get', key: 'secret', appId: 'other' } })
    return answer.ok === false ? `rejected:${answer.error.code}` : 'ALLOWED'
  })
  await attempt('bridge_shell_op', async () => {
    const answer = await internals.invoke('ilmek_bridge', { request: { v: 1, op: 'shell.exec', command: 'id' } })
    return answer.ok === false ? `rejected:${answer.error.code}` : 'ALLOWED'
  })
  await attempt('sdk_replace', async () => { window.ilmek = {}; return window.ilmek === sdk ? 'immutable' : 'REPLACED' })
  await attempt('sdk_redefine', async () => { Object.defineProperty(window, 'ilmek', { value: {} }); return 'REDEFINED' })
  await attempt('bootstrap_hidden', async () => (window.__ILMEK_BOOTSTRAP__ === undefined ? 'hidden' : 'EXPOSED'))

  await attempt('clipboard', async () => String(await navigator.clipboard.readText()))
  await attempt('geolocation', () => new Promise((resolve, reject) => {
    navigator.geolocation.getCurrentPosition(() => { resolve('ALLOWED') }, () => { reject(new Error('denied')) })
  }))
  await attempt('camera', async () => { await navigator.mediaDevices.getUserMedia({ video: true }); return 'ALLOWED' })

  const slug = document.title
  results.local_storage_before = localStorage.getItem('marker') ?? 'none'
  localStorage.setItem('marker', 'mine')
  results.local_storage_marker = localStorage.getItem('marker')
  results.cookie_visible = document.cookie === '' ? 'none' : document.cookie

  await sdk.storage.set('secret', `secret-of-${sdk.app.id}`)
  results.own_secret = await sdk.storage.get('secret')
  results.app_id = sdk.app.id
  results.origin = location.origin

  await sdk.storage.set('report', { title: slug, results })
  document.getElementById('out').textContent = JSON.stringify(results, null, 2)
}

main().catch(async (error) => {
  await sdk.storage.set('report', { title: 'FATAL', results: { fatal: String(error) } })
})

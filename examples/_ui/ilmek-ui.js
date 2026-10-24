'use strict'
/* Small DOM helpers shared by the example applications. Text is only ever set as text. */
window.ui = (() => {
  const $ = (id) => document.getElementById(id)

  function el(tag, props, ...children) {
    const node = document.createElement(tag)
    for (const [key, value] of Object.entries(props ?? {})) {
      if (value === false || value === null || value === undefined) continue
      if (key === 'class') node.className = value
      else if (key === 'text') node.textContent = value
      else if (key.startsWith('on')) node.addEventListener(key.slice(2), value)
      else node.setAttribute(key, value === true ? '' : String(value))
    }
    node.append(...children.filter((child) => child !== null && child !== undefined && child !== false))
    return node
  }

  function hue(text) {
    let hash = 0
    for (const char of String(text)) hash = (hash * 31 + char.codePointAt(0)) >>> 0
    return `h${hash % 12}`
  }

  const initials = (text) => String(text).replace(/[^\p{L}\p{N}]/gu, '').slice(0, 2).toUpperCase() || '?'

  /** Short, stable label for a device id (the last characters are the random part). */
  const who = (id) => String(id).slice(-6)

  const avatar = (id, small = false) =>
    el('span', { class: `avatar ${small ? 'sm ' : ''}${hue(id)}`, 'aria-hidden': 'true', text: initials(who(id)) })

  let toasts = null
  function toast(text, kind = 'info') {
    toasts ??= document.body.appendChild(el('div', { class: 'toasts', role: 'status', 'aria-live': 'polite' }))
    const node = toasts.appendChild(el('div', { class: `toast ${kind}`, text }))
    setTimeout(() => node.remove(), kind === 'error' ? 7000 : 3000)
  }

  const skeleton = () => el('div', { class: 'skeleton', 'aria-hidden': 'true' }, el('span'), el('span'), el('span'))

  function empty(icon, title, text) {
    return el('div', { class: 'empty' },
      el('span', { class: 'empty-art', 'aria-hidden': 'true', text: icon }),
      el('strong', { text: title }),
      text ? el('p', { text }) : null)
  }

  /** Keeps a chip showing the SDK's connection state. */
  function connection(chip) {
    const show = (state) => {
      const ok = state === 'connected'
      chip.className = `chip ${ok ? 'ok' : 'down'}`
      chip.textContent = ok ? 'Bağlı' : 'Bağlantı yok'
    }
    window.ilmek?.connection.state().then(show, () => show('down'))
    window.ilmek?.connection.onChange(show)
  }

  return { $, el, hue, initials, who, avatar, toast, skeleton, empty, connection }
})()

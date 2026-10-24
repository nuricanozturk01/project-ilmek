'use strict'

const sdk = window.ilmek
const { $, el, hue, initials, toast, skeleton, empty, connection } = window.ui
const draftKey = (id) => `draft:${id ?? 'new'}`

let current = null // { id, revision } of the note in the editor, or null for a new note
let notes = []

async function run(action) {
  try { return await action() } catch (error) {
    if (error.code === 'REVISION_CONFLICT') toast('Bu not başka bir yerde değişti. Yeniden yükleyin; yerel taslağınız korunuyor.', 'error')
    else if (error.outcomeUnknown) toast('Sonuç doğrulanamadı; yeniden kontrol ediliyor.', 'error')
    else toast(`Hata: ${error.code}`, 'error')
    return null
  }
}

function showPlaceholder() {
  $('editor').hidden = true
  $('placeholder').hidden = false
  $('placeholder').replaceChildren(empty('✎', 'Bir not seçin', 'Soldan bir not seçin ya da yeni bir not başlatın.'))
}

function renderList() {
  const query = $('filter').value.trim().toLocaleLowerCase('tr')
  const shown = notes.filter((note) => note.title.toLocaleLowerCase('tr').includes(query))
  if (shown.length === 0) {
    $('notes').replaceChildren(el('li', {}, empty('∅', notes.length === 0 ? 'Henüz not yok' : 'Eşleşen not yok')))
    return
  }
  $('notes').replaceChildren(...shown.map((note) =>
    el('li', {},
      el('button', { class: `item${current?.id === note.id ? ' current' : ''}`, type: 'button', onclick: () => openNote(note.id) },
        el('span', { class: `avatar sm ${hue(note.id)}`, 'aria-hidden': 'true', text: initials(note.title) }),
        el('span', { class: 'grow' }, el('span', { class: 'item-title', text: note.title || '(başlıksız)' }))))))
}

async function loadList() {
  if (notes.length === 0) $('notes').replaceChildren(el('li', {}, skeleton()))
  const result = await run(() => sdk.call('notes.list', {}))
  if (!result) return
  notes = result.notes
  renderList()
}

async function saveDraft() {
  const data = new FormData($('editor'))
  await sdk.storage.set(draftKey(current?.id), { title: data.get('title'), body: data.get('body') })
  $('draft-state').textContent = 'Taslak bu cihazda saklandı; sunucuya henüz kaydedilmedi.'
}

async function openNote(id) {
  const note = id === null ? { id: null, title: '', body: '', revision: 0 } : await run(() => sdk.call('notes.get', { id }))
  if (!note) return
  current = id === null ? null : { id: note.id, revision: note.revision }
  const form = $('editor')
  const draft = await sdk.storage.get(draftKey(id))
  form.elements.title.value = draft?.title ?? note.title
  form.elements.body.value = draft?.body ?? note.body
  $('draft-state').textContent = draft ? 'Kaydedilmemiş yerel taslak yüklendi.' : ''
  $('delete').hidden = current === null
  $('placeholder').hidden = true
  form.hidden = false
  form.elements.title.focus()
  renderList()
}

$('new').addEventListener('click', () => openNote(null))
$('filter').addEventListener('input', renderList)
$('editor').addEventListener('input', saveDraft)
$('editor').addEventListener('keydown', (event) => {
  if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) $('editor').requestSubmit()
})

$('editor').addEventListener('submit', async (event) => {
  event.preventDefault()
  const data = new FormData(event.currentTarget)
  const title = data.get('title')
  const body = data.get('body')
  const result = await run(() => current
    ? sdk.call('notes.update', { id: current.id, expected_revision: current.revision, title, body }, { write: true })
    : sdk.call('notes.create', { title, body }, { write: true }))
  if (!result) return
  await sdk.storage.remove(draftKey(current?.id))
  current = { id: result.id, revision: result.revision }
  $('delete').hidden = false
  $('draft-state').textContent = 'Sunucuya kaydedildi.'
  toast('Not kaydedildi.')
  loadList()
})

$('delete').addEventListener('click', async () => {
  if (!current) return
  const button = $('delete')
  if (!button.dataset.armed) {
    // The page runs in a sandbox without modal dialogs, so confirm in place.
    button.dataset.armed = '1'
    button.textContent = 'Silmek için tekrar tıkla'
    setTimeout(() => { delete button.dataset.armed; button.textContent = 'Sil' }, 4000)
    return
  }
  delete button.dataset.armed
  button.textContent = 'Sil'
  const done = await run(() => sdk.call('notes.delete', { id: current.id, expected_revision: current.revision }, { write: true }))
  if (!done) return
  await sdk.storage.remove(draftKey(current.id))
  current = null
  showPlaceholder()
  toast('Not silindi.')
  loadList()
})

connection($('connection'))
showPlaceholder()
loadList()

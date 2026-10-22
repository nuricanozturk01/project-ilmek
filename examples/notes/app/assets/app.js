'use strict'

const sdk = window.ilmek
const $ = (id) => document.getElementById(id)
const notice = (text) => { $('notice').textContent = text }
const draftKey = (id) => `draft:${id ?? 'new'}`

let current = null // { id, revision } of the note in the editor, or null for a new note

async function run(action) {
  notice('')
  try { return await action() } catch (error) {
    if (error.code === 'REVISION_CONFLICT') notice('Bu not başka bir yerde değişti. Yeniden yükleyin; yerel taslağınız korunuyor.')
    else if (error.outcomeUnknown) notice('Sonuç doğrulanamadı; yeniden kontrol ediliyor.')
    else notice(`Hata: ${error.code}`)
    return null
  }
}

async function loadList() {
  const result = await run(() => sdk.call('notes.list', {}))
  if (!result) return
  $('notes').replaceChildren(...result.notes.map((note) => {
    const item = document.createElement('li')
    const open = document.createElement('button')
    open.type = 'button'
    open.textContent = note.title
    open.addEventListener('click', () => openNote(note.id))
    item.append(open)
    return item
  }))
}

async function saveDraft() {
  const form = $('editor')
  const data = new FormData(form)
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
  form.hidden = false
}

$('new').addEventListener('click', () => openNote(null))
$('editor').addEventListener('input', saveDraft)

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
  $('draft-state').textContent = 'Sunucuya kaydedildi.'
  loadList()
})

$('delete').addEventListener('click', async () => {
  if (!current) return
  const done = await run(() => sdk.call('notes.delete', { id: current.id, expected_revision: current.revision }, { write: true }))
  if (!done) return
  await sdk.storage.remove(draftKey(current.id))
  current = null
  $('editor').hidden = true
  loadList()
})

sdk.connection.onChange((state) => { $('connection').textContent = state === 'connected' ? 'Bağlı' : 'Bağlantı yok' })
loadList()

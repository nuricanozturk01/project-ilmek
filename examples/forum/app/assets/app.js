'use strict'

const sdk = window.ilmek
const { $, el, avatar, who, toast, skeleton, empty, connection } = window.ui

let openTopic = null

function describeFailure(error) {
  if (error.outcomeUnknown) return 'Sonuç doğrulanamadı; yeniden kontrol ediliyor. Aynı işlem tekrar gönderilmeyecek.'
  const known = {
    INVALID_MESSAGE: 'Girdiğiniz bilgi geçerli değil.',
    NOT_FOUND: 'Konu bulunamadı.',
    APP_UNAVAILABLE: 'Forum sunucusu şu anda çalışmıyor.',
  }
  return known[error.code] ?? `Hata: ${error.code}`
}

async function run(action) {
  try { return await action() } catch (error) { toast(describeFailure(error), 'error'); return null }
}

function topicRow(topic) {
  const count = topic.comment_count
  return el('li', {},
    el('button', { class: 'item', type: 'button', onclick: () => showTopic(topic.id) },
      avatar(topic.author),
      el('span', { class: 'grow' },
        el('span', { class: 'item-title', text: topic.title }),
        el('span', { class: 'item-text', text: `${who(topic.author)} tarafından` })),
      el('span', { class: `tag topic-count${count === 0 ? '' : ' ok'}`, text: count === 0 ? 'yanıt yok' : `${count} yorum` })))
}

function renderTopics(topics) {
  const list = $('topics')
  if (topics.length === 0) {
    list.replaceChildren(el('li', {}, empty('💬', 'Henüz konu yok', 'İlk konuyu siz açın.')))
    return
  }
  list.replaceChildren(...topics.map(topicRow))
}

async function loadTopics() {
  if ($('topics').childElementCount === 0) $('topics').replaceChildren(el('li', {}, skeleton()))
  const result = await run(() => sdk.call('forum.topics.list', { limit: 50 }))
  if (result) renderTopics(result.topics)
}

async function showTopic(id) {
  const result = await run(() => sdk.call('forum.topic.get', { topic_id: id }))
  if (!result) return
  openTopic = id
  const { topic, comments } = result
  $('topic-title').textContent = topic.title
  $('topic-author').textContent = `${who(topic.author)} tarafından`
  $('topic-avatar').replaceChildren(avatar(topic.author))
  $('topic-body').textContent = topic.body
  $('comments-heading').textContent = comments.length === 0 ? 'Henüz yorum yok' : `${comments.length} yorum`
  $('comments').replaceChildren(...comments.map((comment) =>
    el('li', { class: 'bubble' },
      avatar(comment.author, true),
      el('div', { class: 'bubble-body' },
        el('div', { class: 'meta', text: who(comment.author) }),
        el('div', { text: comment.body })))))
  $('list-view').hidden = true
  $('topic-view').hidden = false
  window.scrollTo(0, 0)
}

function showList() {
  openTopic = null
  $('topic-view').hidden = true
  $('list-view').hidden = false
  loadTopics()
}

function setComposing(on) {
  $('new-topic').hidden = !on
  $('toggle-compose').setAttribute('aria-expanded', String(on))
  if (on) $('new-topic').elements.title.focus()
}

function submitOnCtrlEnter(form) {
  form.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) form.requestSubmit()
  })
}

$('back').addEventListener('click', showList)
$('toggle-compose').addEventListener('click', () => setComposing($('new-topic').hidden))
$('cancel-compose').addEventListener('click', () => setComposing(false))
submitOnCtrlEnter($('new-topic'))
submitOnCtrlEnter($('new-comment'))

$('new-topic').addEventListener('submit', async (event) => {
  event.preventDefault()
  const form = event.currentTarget
  const data = new FormData(form)
  const created = await run(() => sdk.call('forum.topics.create',
    { title: data.get('title'), body: data.get('body') }, { write: true }))
  if (created) { form.reset(); setComposing(false); toast('Konu açıldı.'); loadTopics() }
})

$('new-comment').addEventListener('submit', async (event) => {
  event.preventDefault()
  const form = event.currentTarget
  const body = new FormData(form).get('body')
  const added = await run(() => sdk.call('forum.comments.add', { topic_id: openTopic, body }, { write: true }))
  if (added) { form.reset(); showTopic(openTopic) }
})

connection($('connection'))

// Subscribe first, then load the current state, so no change falls between the two.
sdk.subscribe('forum.events', null, (event) => {
  if (openTopic === null) loadTopics()
  else if (event.topic_id === openTopic) showTopic(openTopic)
}).then(loadTopics, loadTopics)

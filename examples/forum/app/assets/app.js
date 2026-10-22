'use strict'

const sdk = window.ilmek
const $ = (id) => document.getElementById(id)
const notice = (text) => { $('notice').textContent = text }
const short = (author) => author.slice(-8)

let openTopic = null

function describeFailure(error) {
  if (error.outcomeUnknown) return 'Sonuç doğrulanamadı; yeniden kontrol ediliyor. Aynı işlem tekrar gönderilmeyecek.'
  const known = {
    INVALID_MESSAGE: 'Girdiğiniz bilgi geçerli değil.',
    NOT_FOUND: 'Konu bulunamadı.',
    APP_UNAVAILABLE: 'Forum sunucusu şu anda çalışmıyor.',
    TRUST_STALE: 'Topluluk güven bilgisi eski; kayıt sunucusuna erişim gerekiyor.',
  }
  return known[error.code] ?? `Hata: ${error.code}`
}

async function run(action) {
  notice('')
  try { return await action() } catch (error) { notice(describeFailure(error)); return null }
}

function renderTopics(topics) {
  const list = $('topics')
  list.replaceChildren(...topics.map((topic) => {
    const item = document.createElement('li')
    const open = document.createElement('button')
    open.type = 'button'
    open.className = 'link'
    open.textContent = topic.title
    open.addEventListener('click', () => showTopic(topic.id))
    const meta = document.createElement('div')
    meta.className = 'meta'
    meta.textContent = `${short(topic.author)} · ${topic.comment_count} yorum`
    item.append(open, meta)
    return item
  }))
  $('topics-empty').hidden = topics.length > 0
}

async function loadTopics() {
  const result = await run(() => sdk.call('forum.topics.list', { limit: 50 }))
  if (result) renderTopics(result.topics)
}

async function showTopic(id) {
  const result = await run(() => sdk.call('forum.topic.get', { topic_id: id }))
  if (!result) return
  openTopic = id
  $('topic-title').textContent = result.topic.title
  $('topic-body').textContent = result.topic.body
  $('comments').replaceChildren(...result.comments.map((comment) => {
    const item = document.createElement('li')
    item.textContent = comment.body
    const meta = document.createElement('div')
    meta.className = 'meta'
    meta.textContent = short(comment.author)
    item.append(meta)
    return item
  }))
  $('list-view').hidden = true
  $('topic-view').hidden = false
}

function showList() {
  openTopic = null
  $('topic-view').hidden = true
  $('list-view').hidden = false
  loadTopics()
}

$('back').addEventListener('click', showList)

$('new-topic').addEventListener('submit', async (event) => {
  event.preventDefault()
  const form = event.currentTarget
  const data = new FormData(form)
  const created = await run(() => sdk.call('forum.topics.create',
    { title: data.get('title'), body: data.get('body') }, { write: true }))
  if (created) { form.reset(); loadTopics() }
})

$('new-comment').addEventListener('submit', async (event) => {
  event.preventDefault()
  const form = event.currentTarget
  const body = new FormData(form).get('body')
  const added = await run(() => sdk.call('forum.comments.add', { topic_id: openTopic, body }, { write: true }))
  if (added) { form.reset(); showTopic(openTopic) }
})

sdk.connection.onChange((state) => {
  $('connection').textContent = state === 'connected' ? 'Bağlı' : 'Bağlantı yok'
})

// Subscribe first, then load the current state, so no change falls between the two.
sdk.subscribe('forum.events', null, (event) => {
  if (openTopic === null) loadTopics()
  else if (event.topic_id === openTopic) showTopic(openTopic)
}).then(loadTopics)

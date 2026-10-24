'use strict'

let count = 0
document.getElementById('count').addEventListener('click', () => {
  count += 1
  document.getElementById('n').textContent = String(count)
})

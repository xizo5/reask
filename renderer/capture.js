'use strict';
const $ = (id) => document.getElementById(id);

const H_COLLAPSED = 400;
const H_EXPANDED = 520;

const state = {
  pending: [],
  index: 0,
  clipboard: '',
  source: '',
  expanded: false,
  refilling: false,
  hintTimer: 0,
};

const DEFAULT_HINT = '<kbd>Ctrl</kbd><kbd>↵</kbd> 保存，答案以后再补';
const HINT_WITH_ANSWER = '<kbd>Ctrl</kbd><kbd>↵</kbd> 保存并入复习池';
const HINT_REFILL = '<kbd>Ctrl</kbd><kbd>↵</kbd> 保存';

/* ---------------- 视图 ---------------- */

function setExpanded(open) {
  state.expanded = open;
  $('answerBlock').hidden = !open;
  $('ghostAdd').hidden = open;
  $('saveHint').innerHTML = open ? HINT_WITH_ANSWER : DEFAULT_HINT;
  window.api.send('capture:resize', open ? H_HEIGHT() : H_COLLAPSED);
  if (open) setTimeout(() => $('aInput').focus(), 40);
}

function H_HEIGHT() {
  return H_EXPANDED;
}

function renderPending() {
  const n = state.pending.length;
  $('pendingBar').hidden = n === 0;
  $('pendingCount').textContent = String(n);
}

function renderChips() {
  const box = $('chips');
  box.innerHTML = '';

  if (state.source) {
    box.appendChild(chip('来源', truncate(state.source, 42)));
  }
  if (state.clipboard) {
    const lines = state.clipboard.split('\n').filter((l) => l.trim()).length;
    box.appendChild(chip('剪贴板', `${lines} 行`));
  }
}

function chip(k, v) {
  const el = document.createElement('span');
  el.className = 'chip';
  const key = document.createElement('span');
  key.className = 'k';
  key.textContent = k;
  el.appendChild(key);
  el.appendChild(document.createTextNode(' ' + v));
  return el;
}

function truncate(s, n) {
  return s.length > n ? s.slice(0, n) + '…' : s;
}

/* ---------------- 记录 ---------------- */

function resetCapture() {
  state.refilling = false;
  state.index = 0;
  state.source = '';
  $('qInput').value = '';
  $('aInput').value = '';
  $('viewCapture').hidden = false;
  $('viewRefill').hidden = true;
  $('barTitle').innerHTML = '回问 <span>· 新问题</span>';
  $('saveHint').innerHTML = DEFAULT_HINT;
  $('answerBlock').hidden = true;
  $('ghostAdd').hidden = false;
  state.expanded = false;
  window.api.send('capture:resize', H_COLLAPSED);
  renderChips();
  renderPending();
  setTimeout(() => $('qInput').focus(), 30);
  fetchForeground();
}

async function fetchForeground() {
  try {
    const title = await window.api.invoke('capture:foreground');
    if (title && !state.refilling) {
      state.source = title;
      renderChips();
    }
  } catch {}
}

async function saveQuestion() {
  const q = $('qInput').value.trim();
  if (!q) {
    flashHint('先写问题');
    return;
  }
  const res = await window.api.invoke('capture:save', {
    question: q,
    answer: $('aInput').value,
    source: state.source,
    context: state.clipboard,
  });
  if (!res || !res.ok) {
    flashHint(res && res.error ? res.error : '保存失败');
    return;
  }
  window.api.send('capture:hide');
}

function flashHint(text) {
  const el = $('saveHint');
  clearTimeout(state.hintTimer);
  el.innerHTML = text;
  el.classList.add('flash');
  state.hintTimer = setTimeout(() => {
    el.classList.remove('flash');
    el.innerHTML = state.expanded ? HINT_WITH_ANSWER : DEFAULT_HINT;
  }, 1800);
}

/* ---------------- 回填 ---------------- */

function openRefill() {
  if (!state.pending.length) return;
  state.refilling = true;
  state.index = 0;
  $('viewCapture').hidden = true;
  $('viewRefill').hidden = false;
  window.api.send('capture:resize', H_COLLAPSED);
  renderRefill();
}

function renderRefill() {
  const item = state.pending[state.index];
  if (!item) {
    backToCapture();
    return;
  }
  $('rfQuestion').textContent = item.question;
  $('rfMeta').textContent = [item.source, fmtTime(item.created_at)].filter(Boolean).join('  /  ');
  $('rfAnswer').value = '';
  $('barTitle').innerHTML = `回问 <span>· 回填 ${state.index + 1} / ${state.pending.length}</span>`;
  setTimeout(() => $('rfAnswer').focus(), 30);
}

function backToCapture() {
  state.refilling = false;
  $('viewCapture').hidden = false;
  $('viewRefill').hidden = true;
  $('barTitle').innerHTML = '回问 <span>· 新问题</span>';
  setTimeout(() => $('qInput').focus(), 30);
}

async function saveRefill(next) {
  const item = state.pending[state.index];
  if (!item) return backToCapture();

  if (!next) {
    const answer = $('rfAnswer').value.trim();
    if (!answer) return backToCapture();
    await window.api.invoke('refill:save', { id: item.id, answer });
  }

  state.pending.splice(state.index, 1);
  renderPending();

  if (!state.pending.length) return backToCapture();
  if (state.index >= state.pending.length) state.index = state.pending.length - 1;
  renderRefill();
}

/* ---------------- 工具 ---------------- */

function fmtTime(iso) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const now = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  const sameDay = d.toDateString() === now.toDateString();
  return sameDay
    ? `${pad(d.getHours())}:${pad(d.getMinutes())}`
    : `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/* ---------------- 事件 ---------------- */

$('ghostAdd').addEventListener('click', () => setExpanded(true));
$('pendingBar').addEventListener('click', openRefill);
$('btnHistory').addEventListener('click', () => window.api.send('capture:openHistory'));
$('rfSkip').addEventListener('click', () => saveRefill(true));
$('rfSave').addEventListener('click', () => saveRefill(false));

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    e.preventDefault();
    window.api.send('capture:hide');
    return;
  }

  if (e.key === 'Tab' && !state.refilling && !state.expanded) {
    e.preventDefault();
    setExpanded(true);
    return;
  }

  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    if (state.refilling) saveRefill(false);
    else saveQuestion();
  }
});

window.api.on('capture:reset', () => boot());

/* ---------------- 启动 ---------------- */

async function boot() {
  resetCapture();
  try {
    const init = await window.api.invoke('capture:init');
    state.pending = Array.isArray(init.pending) ? init.pending : [];
    state.clipboard = init.clipboard || '';
    if (init.hotkey) {
      $('hotkeyLabel').textContent = String(init.hotkey).replace(/Control/g, 'Ctrl');
    }
    renderPending();
    renderChips();
  } catch (e) {
    console.error(e);
  }
}

boot();

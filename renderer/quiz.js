'use strict';
const $ = (id) => document.getElementById(id);

const state = {
  kind: 'daily',
  items: [],
  index: 0,
  answers: {},
  busy: false,
};

const KIND_LABEL = { daily: '日卷', mixed: '大杂烩' };

/* ---------------- 视图 ---------------- */

function showLoading(sub) {
  $('loading').hidden = false;
  $('stage').hidden = true;
  $('empty').hidden = true;
  $('loadingSub').textContent = sub || '';
}

function showEmpty() {
  $('loading').hidden = true;
  $('stage').hidden = true;
  $('empty').hidden = false;
}

function start(data) {
  state.kind = data.kind || 'daily';
  state.items = Array.isArray(data.items) ? data.items : [];
  state.index = 0;
  state.answers = {};
  state.busy = false;

  if (!state.items.length) {
    showEmpty();
    return;
  }

  $('loading').hidden = true;
  $('empty').hidden = true;
  $('stage').hidden = false;
  render();
}

function render() {
  const item = state.items[state.index];
  const total = state.items.length;
  const isLast = state.index === total - 1;

  const d = new Date();
  $('barTitle').innerHTML = `${KIND_LABEL[state.kind] || '卷'} <span>· ${d.getMonth() + 1} 月 ${d.getDate()} 日</span>`;
  $('counter').innerHTML = `<b>${state.index + 1}</b> / ${total}`;
  $('qNum').textContent = String(state.index + 1).padStart(2, '0');
  $('qText').textContent = item.prompt;
  $('aInput').value = state.answers[item.id] || '';

  const pct = ((state.index + 1) / total) * 100;
  $('progressFill').style.width = `${pct}%`;

  $('btnNext').textContent = isLast ? '提交试卷' : '下一题';
  $('hintText').textContent = isLast ? '提交试卷' : '下一题';
  $('hintText').style.color = '';
  $('btnSkip').textContent = isLast ? '这题跳过并提交' : '这题跳过';

  setTimeout(() => $('aInput').focus(), 20);
}

/* ---------------- 交互 ---------------- */

function collect() {
  const item = state.items[state.index];
  state.answers[item.id] = $('aInput').value.trim();
}

function next() {
  collect();
  if (state.index < state.items.length - 1) {
    state.index += 1;
    render();
  } else {
    submit();
  }
}

function skip() {
  const item = state.items[state.index];
  state.answers[item.id] = '';
  if (state.index < state.items.length - 1) {
    state.index += 1;
    render();
  } else {
    submit();
  }
}

async function submit() {
  if (state.busy) return;
  state.busy = true;

  $('progressFill').style.width = '100%';
  $('stage').hidden = true;
  $('loading').hidden = false;
  $('loadingSub').textContent = 'AI 正在逐题判分…';
  document.querySelector('.loading .txt').textContent = '判分中…';

  try {
    const res = await window.api.invoke('quiz:submit', { answers: state.answers });
    if (!res || !res.ok) throw new Error((res && res.error) || '提交失败');
    // 主进程会打开结果窗口并关闭本窗口
  } catch (err) {
    state.busy = false;
    $('loading').hidden = true;
    $('stage').hidden = false;
    document.querySelector('.loading .txt').textContent = '正在出卷…';
    $('loadingSub').textContent = '';
    $('hintText').textContent = '提交失败，重试';
    $('hintText').style.color = 'var(--err)';
    console.error(err);
  }
}

/* ---------------- 事件 ---------------- */

$('btnNext').addEventListener('click', next);
$('btnSkip').addEventListener('click', skip);
$('btnClose').addEventListener('click', () => window.api.send('win:close'));

$('aInput').addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    next();
  }
});

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') window.api.send('win:close');
});

window.api.on('quiz:ready', (data) => start(data));

/* ---------------- 启动 ---------------- */

(async function boot() {
  showLoading();
  try {
    const data = await window.api.invoke('quiz:get');
    if (data && data.items && data.items.length) start(data);
  } catch (e) {
    showEmpty();
  }
})();

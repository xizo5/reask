'use strict';
const $ = (id) => document.getElementById(id);

let recording = false;
let saveTimer = 0;

/* ---------------- 显示 ---------------- */

function prettyHotkey(acc) {
  return String(acc || '')
    .replace(/Control/g, 'Ctrl')
    .replace(/\+/g, ' + ');
}

function fill(cfg) {
  $('hotkeyView').textContent = prettyHotkey(cfg.hotkey);
  $('quizTime').value = cfg.quizTime || '21:00';
  $('mixedIntervalDays').value = cfg.mixedIntervalDays;
  $('mixedLimit').value = cfg.mixedLimit;
  $('pendingDays').value = cfg.pendingDays;
  $('apiKey').value = cfg.apiKey || '';
  $('baseUrl').value = cfg.baseUrl || '';
  $('model').value = cfg.model || '';
  $('autoBackup').setAttribute('aria-checked', cfg.autoBackup ? 'true' : 'false');

  if (cfg.stats) {
    const s = cfg.stats;
    $('statsLine').textContent =
      `共 ${s.total} 条记录，${s.answered} 条有答案，做过 ${s.sessions} 次卷` +
      (s.rate === null ? '' : `，正确率 ${s.rate}%`);
  }
}

/* ---------------- 保存 ---------------- */

function stamp() {
  const el = $('saved');
  el.classList.add('on');
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => el.classList.remove('on'), 1400);
}

async function patch(obj) {
  await window.api.invoke('settings:set', obj);
  stamp();
}

function numberOr(id, fallback) {
  const v = Number($(id).value);
  return Number.isFinite(v) && v > 0 ? v : fallback;
}

/* ---------------- 热键录制 ---------------- */

const MODIFIER_ONLY = new Set(['Control', 'Shift', 'Alt', 'Meta', 'CapsLock', 'Escape']);

function startRecord() {
  recording = true;
  $('btnRecord').textContent = '按键…';
  $('hotkeyView').textContent = '按下组合键';
  $('footHint').textContent = '按 Esc 取消录制';
}

function stopRecord() {
  recording = false;
  $('btnRecord').textContent = '录制';
  $('footHint').textContent = '改动即时保存';
}

async function captureCombo(e) {
  e.preventDefault();

  if (e.key === 'Escape') {
    stopRecord();
    const cfg = await window.api.invoke('settings:get');
    $('hotkeyView').textContent = prettyHotkey(cfg.hotkey);
    return;
  }

  if (MODIFIER_ONLY.has(e.key)) return;

  const parts = [];
  if (e.ctrlKey) parts.push('Control');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push('Super');

  let key = e.key;
  if (key === ' ') key = 'Space';
  else if (key.length === 1) key = key.toUpperCase();
  else if (key === 'ArrowUp') key = 'Up';
  else if (key === 'ArrowDown') key = 'Down';
  else if (key === 'ArrowLeft') key = 'Left';
  else if (key === 'ArrowRight') key = 'Right';

  parts.push(key);
  const acc = parts.join('+');

  if (parts.length < 2) {
    $('hotkeyView').textContent = '至少要带一个修饰键';
    return;
  }

  stopRecord();
  $('hotkeyView').textContent = prettyHotkey(acc);
  await patch({ hotkey: acc });
}

/* ---------------- 事件 ---------------- */

$('btnRecord').addEventListener('click', () => (recording ? stopRecord() : startRecord()));

document.addEventListener('keydown', (e) => {
  if (recording) {
    captureCombo(e);
    return;
  }
  if (e.key === 'Escape') window.api.send('win:close');
});

$('quizTime').addEventListener('change', () => patch({ quizTime: $('quizTime').value || '21:00' }));

$('mixedIntervalDays').addEventListener('change', () =>
  patch({ mixedIntervalDays: numberOr('mixedIntervalDays', 3) })
);

$('mixedLimit').addEventListener('change', () =>
  patch({ mixedLimit: numberOr('mixedLimit', 20) })
);

$('pendingDays').addEventListener('change', () =>
  patch({ pendingDays: numberOr('pendingDays', 7) })
);

$('apiKey').addEventListener('change', () => patch({ apiKey: $('apiKey').value.trim() }));
$('baseUrl').addEventListener('change', () => patch({ baseUrl: $('baseUrl').value.trim() }));
$('model').addEventListener('change', () => patch({ model: $('model').value.trim() }));

$('autoBackup').addEventListener('click', () => {
  const next = $('autoBackup').getAttribute('aria-checked') !== 'true';
  $('autoBackup').setAttribute('aria-checked', next ? 'true' : 'false');
  patch({ autoBackup: next });
});

$('btnDir').addEventListener('click', () => window.api.invoke('app:openDataDir'));

$('btnBackup').addEventListener('click', async () => {
  const res = await window.api.invoke('app:backup');
  $('footHint').textContent = res && res.ok ? '备份已写入 backup 目录' : '备份失败：' + (res && res.error);
  setTimeout(() => ($('footHint').textContent = '改动即时保存'), 2600);
});

$('btnClose').addEventListener('click', () => window.api.send('win:close'));

/* ---------------- 启动 ---------------- */

(async function boot() {
  const cfg = await window.api.invoke('settings:get');
  fill(cfg);
})();

'use strict';
const $ = (id) => document.getElementById(id);

const state = {
  scope: 'all',
  keyword: '',
  rows: [],
  counts: {},
};

let searchTimer = 0;

const SCOPE_HINT = {
  today: ['今天还没记东西', '按 Ctrl+Shift+Q 记一条，就会出现在这儿'],
  all: ['一条都还没有', '按 Ctrl+Shift+Q 记第一条'],
  pending: ['没有待回填的', '记了但没填答案的题会挂在这里，最多留 7 天'],
  archived: ['已放弃是空的', '超过保留天数还没填答案的题会折叠到这里，不删不考'],
};

/* ---------------- 渲染 ---------------- */

function tagOf(row) {
  if (row.archived === 1) return ['tag-err', '已放弃'];
  if (!row.answer || !String(row.answer).trim()) return ['tag-warn', '待回填'];
  return ['tag-ok', '有答案'];
}

function render() {
  const list = $('list');
  list.innerHTML = '';

  const rows = state.rows;
  $('empty').hidden = rows.length > 0;
  list.style.display = rows.length ? 'block' : 'none';

  if (!rows.length) {
    const [t, d] = SCOPE_HINT[state.scope] || SCOPE_HINT.all;
    $('emptyTitle').textContent = state.keyword ? '没搜到' : t;
    $('emptyDesc').textContent = state.keyword ? `换个词试试，或清空搜索框` : d;
  }

  for (const row of rows) list.appendChild(rowEl(row));

  const total = rows.length;
  const asked = rows.filter((r) => r.answer && String(r.answer).trim()).length;
  $('footNote').textContent =
    `${total} 条${state.keyword ? ' 匹配' : ''}` + (asked !== total ? ` · ${asked} 条有答案` : '');
}

function rowEl(row) {
  const el = document.createElement('div');
  el.className = 'row';

  const head = document.createElement('div');
  head.className = 'rowhead';

  const q = document.createElement('p');
  q.className = 'q';
  q.textContent = row.question;
  head.appendChild(q);

  const [cls, text] = tagOf(row);
  const tag = document.createElement('span');
  tag.className = `tag ${cls}`;
  tag.textContent = text;
  head.appendChild(tag);

  const meta = document.createElement('div');
  meta.className = 'meta';
  const bits = [];
  if (row.source) bits.push(row.source);
  bits.push(fmtTime(row.created_at));
  if (row.ask_count > 0) bits.push(`问过 ${row.ask_count} 次`);
  if (row.wrong_count > 0) bits.push(`错过 ${row.wrong_count} 次`);
  bits.forEach((b, i) => {
    if (i) {
      const s = document.createElement('span');
      s.className = 'sep';
      s.textContent = '/';
      meta.appendChild(s);
    }
    const span = document.createElement('span');
    span.textContent = b;
    meta.appendChild(span);
  });

  const detail = document.createElement('div');
  detail.className = 'detail';
  detail.hidden = true;

  const lb = document.createElement('div');
  lb.className = 'lb';
  lb.textContent = '答案';
  detail.appendChild(lb);

  const ans = document.createElement('p');
  if (row.answer && String(row.answer).trim()) {
    ans.textContent = row.answer;
  } else {
    ans.className = 'none';
    ans.textContent = '还没填。找到答案后按 Ctrl+Shift+Q，点顶部那条提示去补。';
  }
  detail.appendChild(ans);

  if (row.context) {
    const ctx = document.createElement('div');
    ctx.className = 'ctx';
    ctx.textContent = row.context;
    detail.appendChild(ctx);
  }

  el.appendChild(head);
  el.appendChild(meta);
  el.appendChild(detail);

  el.addEventListener('click', () => {
    el.classList.toggle('open');
    detail.hidden = !el.classList.contains('open');
  });

  return el;
}

function fmtTime(iso) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const now = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  const sameDay = d.toDateString() === now.toDateString();
  if (sameDay) return `今天 ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const y = new Date(now);
  y.setDate(now.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return `昨天 ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  if (d.getFullYear() === now.getFullYear()) {
    return `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function renderCounts() {  for (const scope of ['today', 'all', 'pending', 'archived']) {
    const el = $(`n-${scope}`);
    if (el) el.textContent = String(state.counts[scope] ?? 0);
  }
}

/* ---------------- 数据 ---------------- */

async function load() {
  const data = await window.api.invoke('history:list', {
    scope: state.scope,
    keyword: state.keyword,
  });
  state.rows = (data && data.rows) || [];
  state.counts = (data && data.counts) || {};
  renderCounts();
  render();
}

/* ---------------- 交互 ---------------- */

$('tabs').addEventListener('click', (e) => {
  const btn = e.target.closest('.tab');
  if (!btn) return;
  state.scope = btn.dataset.scope;
  for (const t of document.querySelectorAll('.tab')) t.classList.toggle('on', t === btn);
  load();
});

$('search').addEventListener('input', () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    state.keyword = $('search').value.trim();
    load();
  }, 180);
});

$('btnClose').addEventListener('click', () => window.api.send('win:close'));

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') window.api.send('win:close');
});

/* ---------------- 启动 ---------------- */

(async function boot() {
  const first = document.querySelector('.tab[data-scope="all"]');
  if (first) first.classList.add('on');
  await load();
})();

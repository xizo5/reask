'use strict';
const $ = (id) => document.getElementById(id);

const ICON_OK =
  '<svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.2"><path d="M3 8.5l3.4 3.4L13 5"/></svg>';
const ICON_PARTIAL =
  '<svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><circle cx="8" cy="8" r="5.5"/><path d="M8 4.5v3.8"/><path d="M8 11h.01"/></svg>';
const ICON_WRONG =
  '<svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.2"><path d="M4 4l8 8M12 4l-8 8"/></svg>';

const VERDICT = {
  ok: { cls: 'tag-ok', text: '对', icon: ICON_OK },
  partial: { cls: 'tag-warn', text: '部分对', icon: ICON_PARTIAL },
  wrong: { cls: 'tag-err', text: '错', icon: ICON_WRONG },
};

const KIND_LABEL = { daily: '日卷', mixed: '大杂烩' };

let collapsed = false;

function render(data) {
  const { kind, score, detail, offline, nextMixedDue } = data;
  const total = (score.ok || 0) + (score.partial || 0) + (score.wrong || 0);

  const d = new Date();
  $('barTitle').innerHTML = `判分 <span>· ${KIND_LABEL[kind] || '卷'} ${d.getMonth() + 1} 月 ${d.getDate()} 日</span>`;
  $('scoreNum').textContent = String(score.ok || 0);
  $('scoreUnit').textContent = `/ ${total} 答对`;

  const tags = $('scoreTags');
  tags.innerHTML = '';
  if (score.ok) tags.appendChild(tagHtml('ok', score.ok));
  if (score.partial) tags.appendChild(tagHtml('partial', score.partial));
  if (score.wrong) tags.appendChild(tagHtml('wrong', score.wrong));

  $('offlineSlot').innerHTML = offline
    ? '<span class="offline-note">离线判分 · 配 API Key 后更准</span>'
    : '';

  const list = $('list');
  list.innerHTML = '';
  detail.forEach((item, i) => list.appendChild(itemHtml(item, i)));

  const wrongish = (score.partial || 0) + (score.wrong || 0);
  $('footNote').textContent = wrongish
    ? `${wrongish} 道题已加重，下次会优先抽到`
    : '全对，这些题下次会隔更久才出现';

  const btnMixed = $('btnMixed');
  if (nextMixedDue && nextMixedDue.due) {
    btnMixed.hidden = false;
    btnMixed.textContent = '还有大杂烩';
  } else {
    btnMixed.hidden = true;
  }
}

function tagHtml(verdict, n) {
  const v = VERDICT[verdict];
  const span = document.createElement('span');
  span.className = `tag ${v.cls}`;
  span.innerHTML = `${v.icon}${v.text} ${n}`;
  return span;
}

function itemHtml(item, i) {
  const v = VERDICT[item.verdict] || VERDICT.partial;

  const row = document.createElement('div');
  row.className = 'item';

  const q = document.createElement('p');
  q.className = 'q';
  const idx = document.createElement('span');
  idx.className = 'idx';
  idx.textContent = String(i + 1).padStart(2, '0');
  q.appendChild(idx);
  q.appendChild(document.createTextNode(item.prompt));

  const tag = document.createElement('span');
  tag.className = `tag ${v.cls}`;
  tag.innerHTML = `${v.icon}${v.text}`;

  const both = document.createElement('div');
  both.className = 'both';

  const mine = document.createElement('div');
  mine.className = 'col';
  mine.innerHTML = '<div class="lb">你的答案</div>';
  const p1 = document.createElement('p');
  if (item.user) p1.textContent = item.user;
  else {
    p1.className = 'none';
    p1.textContent = '（空白）';
  }
  mine.appendChild(p1);

  const key = document.createElement('div');
  key.className = 'col key';
  key.innerHTML = '<div class="lb">标准答案</div>';
  const p2 = document.createElement('p');
  p2.textContent = item.key || '—';
  key.appendChild(p2);

  if (item.verdict !== 'ok' && item.reason) {
    const why = document.createElement('div');
    why.className = 'why';
    why.textContent = item.reason;
    key.appendChild(why);
  }

  both.appendChild(mine);
  both.appendChild(key);

  row.appendChild(q);
  row.appendChild(tag);
  row.appendChild(both);
  return row;
}

/* ---------------- 交互 ---------------- */

$('btnToggle').addEventListener('click', () => {
  collapsed = !collapsed;
  $('list').style.display = collapsed ? 'none' : 'flex';
  $('btnToggle').textContent = collapsed ? '展开详情' : '收起详情';
});

$('btnDone').addEventListener('click', () => window.api.send('win:close'));

$('btnMixed').addEventListener('click', () => {
  window.api.send('quiz:start', 'mixed');
  window.api.send('win:close');
});

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') window.api.send('win:close');
});

/* ---------------- 启动 ---------------- */

(async function boot() {
  const data = await window.api.invoke('result:get');
  if (data) render(data);
})();

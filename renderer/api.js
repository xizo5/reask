'use strict';
/**
 * 前端与 Tauri 之间的唯一适配层。
 * 五个页面的业务代码一行没改，它们仍然调用 window.api.invoke / send / on，
 * 这里把这三个方法翻译成 Tauri 的 command 和 event。
 */

(function () {
  function failLoud(reason) {
    try {
      const pre = document.createElement('pre');
      pre.style.cssText =
        'color:#E06C75;font:13px/1.7 Consolas,monospace;padding:24px;white-space:pre-wrap;margin:0';
      pre.textContent = '回问启动失败\n\n' + reason;
      document.body.innerHTML = '';
      document.body.style.background = '#12151A';
      document.body.appendChild(pre);
    } catch (e) {
      /* 连这个都失败就没办法了 */
    }
  }

  if (!window.__TAURI__) {
    failLoud('window.__TAURI__ 不存在。\nwithGlobalTauri 没生效，或者页面脚本被拦了。');
    return;
  }

  const INVOKE_MAP = {
    'capture:init': 'capture_init',
    'capture:foreground': 'capture_foreground',
    'capture:save': 'capture_save',
    'refill:save': 'refill_save',
    'history:list': 'history_list',
    'quiz:get': 'quiz_get',
    'quiz:submit': 'quiz_submit',
    'result:get': 'result_get',
    'settings:get': 'settings_get',
    'settings:set': 'settings_set',
    'app:openDataDir': 'app_open_data_dir',
    'app:backup': 'app_backup',
    'app:quit': 'app_quit',
  };

  const SEND_MAP = {
    'capture:resize': 'capture_resize',
    'capture:hide': 'capture_hide',
    'capture:openHistory': 'capture_open_history',
    'quiz:start': 'quiz_start',
    'win:close': 'win_close',
  };

  /** Rust 命令的参数名和前端传的对象形状不完全一致，这里做一次搬运 */
  const ADAPT = {
    'capture:save': (d) => ({ payload: d }),
    'settings:set': (d) => ({ patch: d }),
    'capture:resize': (d) => ({ height: d }),
  };

  const core = window.__TAURI__.core;
  const event = window.__TAURI__.event;

  function call(cmd, args) {
    return core.invoke(cmd, args || {});
  }

  window.api = {
    invoke(channel, data) {
      const cmd = INVOKE_MAP[channel];
      if (!cmd) return Promise.reject(new Error('blocked invoke: ' + channel));
      const adapt = ADAPT[channel];
      return call(cmd, adapt ? adapt(data) : data);
    },

    send(channel, data) {
      const cmd = SEND_MAP[channel];
      if (!cmd) throw new Error('blocked send: ' + channel);
      const adapt = ADAPT[channel];
      call(cmd, adapt ? adapt(data) : data || {}).catch((e) =>
        console.error('[send]', channel, e)
      );
    },

    on(channel, cb) {
      event.listen(channel, (e) => cb(e.payload)).catch((err) =>
        console.error('[listen]', channel, err)
      );
    },
  };

  /* WebView2 不认 -webkit-app-region，用 Tauri 的拖拽区域属性代替 */
  document.addEventListener('DOMContentLoaded', () => {
    const bar = document.querySelector('.bar');
    if (!bar) return;
    bar.setAttribute('data-tauri-drag-region', '');
    bar.querySelectorAll('.dot, .title, .dash').forEach((el) => {
      el.setAttribute('data-tauri-drag-region', '');
    });
  });

  /* 自检：把渲染层的实际情况报给 Rust，黑屏时终端能看到原因 */
  window.addEventListener('error', (e) => {
    try {
      core.invoke('renderer_ready', {
        info: {
          kind: 'error',
          message: String(e.message),
          file: String(e.filename),
          line: e.lineno,
        },
      });
    } catch (_) {}
  });

  window.addEventListener('load', () => {
    try {
      const app = document.querySelector('.app');
      const body = document.body;
      core.invoke('renderer_ready', {
        info: {
          kind: 'load',
          href: location.href,
          vw: window.innerWidth,
          vh: window.innerHeight,
          bodyH: body ? Math.round(body.getBoundingClientRect().height) : -1,
          appH: app ? Math.round(app.getBoundingClientRect().height) : -1,
          children: app ? app.children.length : -1,
        },
      });
    } catch (_) {}
  });
})();

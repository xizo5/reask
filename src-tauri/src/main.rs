#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod fg;
mod rules;
mod settings;
mod store;

use ai::{GradeInput, QuizItem};
use serde::Deserialize;
use serde_json::{json, Value};
use settings::Settings;
use std::sync::Mutex;
use std::time::Duration;
use store::Store;
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const CAP_W: f64 = 600.0;
const CAP_H: f64 = 400.0;
const CAP_H_OPEN: f64 = 520.0;

struct ActiveSession {
    id: i64,
    kind: String,
    items: Vec<QuizItem>,
}

struct AppState {
    store: Store,
    settings: Mutex<Settings>,
    http: reqwest::Client,
    session: Mutex<Option<ActiveSession>>,
    result: Mutex<Option<Value>>,
    data_dir: std::path::PathBuf,
    /// 弹窗弹出「之前」的前台窗口。必须在 show() 之前抓，
    /// 否则抓到的永远是回问自己。
    last_fg: Mutex<String>,
    /// 首次 show 需要额外触发一次重绘
    first_show: Mutex<bool>,
}

fn cfg_of(state: &AppState) -> Settings {
    state
        .settings
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

/* ============================================================
   窗口
   ============================================================ */

fn ensure_capture(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("capture") {
        return Ok(w);
    }
    // 关键：必须以「可见」状态创建一次再立刻藏起来。
    // 直接 visible(false) 创建的窗口，WebView2 不会初始化绘制，
    // 后面再 show() 就是一片黑（DOM 明明正常，就是不上屏）。
    let w = WebviewWindowBuilder::new(app, "capture", WebviewUrl::App("capture.html".into()))
        .title("回问")
        .inner_size(CAP_W, CAP_H)
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .background_color(tauri::window::Color(18, 21, 26, 255))
        .visible(true)
        .build()
        .map_err(|e| e.to_string())?;

    let _ = w.hide();

    let handle = w.clone();
    w.on_window_event(move |e| {
        if let WindowEvent::Focused(false) = e {
            let _ = handle.hide();
        }
    });
    Ok(w)
}

fn show_capture(app: &AppHandle) -> Result<(), String> {
    let w = ensure_capture(app)?;

    // 关键顺序：先抓前台窗口，再 show()。
    // 反过来抓到的就是「回问 — reask」自己。
    // 若弹窗本身已经在前台（连按两次热键），保留上一次的来源，不要覆盖成自己。
    let before = fg::foreground_title();
    if !before.is_empty() && !before.ends_with("— reask") {
        if let Some(state) = app.try_state::<AppState>() {
            if let Ok(mut g) = state.last_fg.lock() {
                *g = before;
            }
        }
    }

    let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(CAP_W, CAP_H)));

    if let Ok(Some(mon)) = w.current_monitor() {
        let scale = mon.scale_factor();
        let size = mon.size();
        let pos = mon.position();
        let px_w = (CAP_W * scale) as i32;
        let x = pos.x + (size.width as i32 - px_w) / 2;
        let y = pos.y + (size.height as f64 * 0.16) as i32;
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }

    w.show().map_err(|e| e.to_string())?;
    w.set_focus().map_err(|e| e.to_string())?;

    // 首次显示时再强制重绘一次：WebView2 偶尔会在 show 后停在空白帧
    if let Some(state) = app.try_state::<AppState>() {
        let need_nudge = state
            .first_show
            .lock()
            .map(|mut g| {
                let was = *g;
                *g = false;
                was
            })
            .unwrap_or(false);
        if need_nudge {
            if let Ok(s) = w.inner_size() {
                let _ = w.set_size(tauri::PhysicalSize::new(s.width + 1, s.height));
                std::thread::sleep(Duration::from_millis(16));
                let _ = w.set_size(tauri::PhysicalSize::new(s.width, s.height));
            }
        }
    }

    let _ = w.emit("capture:reset", ());
    Ok(())
}

fn open_aux(
    app: &AppHandle,
    label: &str,
    file: &str,
    w: f64,
    h: f64,
) -> Result<WebviewWindow, String> {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(win);
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(file.into()))
        .title("回问")
        .inner_size(w, h)
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .build()
        .map_err(|e| e.to_string())
}

fn open_history(app: &AppHandle) -> Result<(), String> {
    open_aux(app, "history", "history.html", 900.0, 660.0).map(|_| ())
}

fn open_settings(app: &AppHandle) -> Result<(), String> {
    open_aux(app, "settings", "settings.html", 600.0, 600.0).map(|_| ())
}

/* ============================================================
   热键
   ============================================================ */

fn code_of(key: &str) -> Option<Code> {
    Some(match key {
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "space" => Code::Space,
        "escape" => Code::Escape,
        "enter" => Code::Enter,
        "tab" => Code::Tab,
        "up" => Code::ArrowUp,
        "down" => Code::ArrowDown,
        "left" => Code::ArrowLeft,
        "right" => Code::ArrowRight,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        _ => return None,
    })
}

fn parse_hotkey(s: &str) -> Option<Shortcut> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in s.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "control" | "ctrl" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "alt" => mods |= Modifiers::ALT,
            "super" | "meta" | "cmd" | "win" => mods |= Modifiers::SUPER,
            other => code = code_of(other),
        }
    }
    Some(Shortcut::new(
        if mods.is_empty() { None } else { Some(mods) },
        code?,
    ))
}

fn register_hotkey(app: &AppHandle, cfg: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let sc = parse_hotkey(&cfg.hotkey).ok_or_else(|| format!("无法识别热键 {}", cfg.hotkey))?;
    gs.register(sc)
        .map_err(|e| format!("热键 {} 注册失败：{}", cfg.hotkey, e))
}

/* ============================================================
   托盘
   ============================================================ */

fn refresh_tray(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let cfg = cfg_of(&state);
    let pending = state.store.count_pending(cfg.pending_days);
    let due = rules::mixed_due(&state.store, &cfg);

    let quiz_label = if pending > 0 {
        format!("开始今天的卷（{} 条待回填）", pending)
    } else {
        "开始今天的卷".to_string()
    };
    let mixed_label = if due.due {
        "大杂烩：可以做".to_string()
    } else {
        format!("大杂烩：{}", due.reason)
    };

    let mk = |id: &str, label: &str| -> Result<tauri::menu::MenuItem<tauri::Wry>, String> {
        MenuItemBuilder::with_id(id, label)
            .build(app)
            .map_err(|e| e.to_string())
    };

    let menu = MenuBuilder::new(app)
        .item(&mk("capture", "记一个问题")?)
        .item(&mk("history", "全部问题")?)
        .separator()
        .item(&mk("quiz", &quiz_label)?)
        .item(
            &MenuItemBuilder::with_id("mixed", &mixed_label)
                .enabled(due.due)
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .separator()
        .item(&mk("settings", "设置")?)
        .item(&mk("dir", "打开数据目录")?)
        .separator()
        .item(&mk("quit", "退出")?)
        .build()
        .map_err(|e| e.to_string())?;

    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn open_path(p: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(p)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn build_tray(app: &AppHandle) -> Result<(), String> {
    let icon = Image::from_bytes(include_bytes!("../icons/tray.png")).map_err(|e| e.to_string())?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("回问")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "capture" => {
                let _ = show_capture(app);
            }
            "history" => {
                let _ = open_history(app);
            }
            "quiz" => {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    start_quiz(a.clone(), None).await;
                });
            }
            "mixed" => {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    start_quiz(a, Some("mixed".to_string())).await;
                });
            }
            "settings" => {
                let _ = open_settings(app);
            }
            "dir" => {
                let state = app.state::<AppState>();
                let _ = open_path(&state.data_dir);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = show_capture(tray.app_handle());
            }
        })
        .build(app)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/* ============================================================
   通知
   ============================================================ */

fn notify(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body.to_string())
        .show();
}

/* ============================================================
   出卷 / 判分
   ============================================================ */

async fn start_quiz(app: AppHandle, forced: Option<String>) {
    let state = app.state::<AppState>();
    let cfg = cfg_of(&state);

    let (kind, rows) = match forced {
        Some(k) => {
            let rows = rules::rows_for(&state.store, &cfg, &k).unwrap_or_default();
            (k, rows)
        }
        None => {
            let picked = match rules::pick_session(&state.store, &cfg) {
                Ok(p) => p,
                Err(_) => return,
            };
            match picked.kind {
                Some(k) => (k, picked.rows),
                None => {
                    notify(&app, "今天没有要考的题", &picked.reason);
                    return;
                }
            }
        }
    };

    if rows.is_empty() {
        notify(&app, "没有可考的题", "先去记几道题吧");
        return;
    }

    if let Err(e) = open_aux(&app, "quiz", "quiz.html", 820.0, 520.0) {
        notify(&app, "打开卷子失败", &e);
        return;
    }

    let items = ai::build_quiz(&cfg, &state.http, &rows).await;
    let ids: Vec<i64> = items.iter().map(|i| i.id).collect();
    let _ = state.store.mark_asked(&ids);
    let payload = serde_json::to_value(&items).unwrap_or(json!([]));
    let sid = state.store.create_session(&kind, &payload).unwrap_or(0);

    if let Ok(mut g) = state.session.lock() {
        *g = Some(ActiveSession {
            id: sid,
            kind: kind.clone(),
            items: items.clone(),
        });
    }

    if let Some(w) = app.get_webview_window("quiz") {
        let _ = w.emit("quiz:ready", json!({ "kind": kind, "items": items }));
    }
}

async fn submit_quiz(
    app: &AppHandle,
    answers: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let cfg = cfg_of(&state);

    let (session_id, kind, items) = {
        let g = state.session.lock().map_err(|e| e.to_string())?;
        match g.as_ref() {
            Some(s) => (s.id, s.kind.clone(), s.items.clone()),
            None => return Err("没有正在进行的卷子".into()),
        }
    };

    let inputs: Vec<GradeInput> = items
        .iter()
        .map(|i| GradeInput {
            id: i.id,
            prompt: i.prompt.clone(),
            key: i.key.clone(),
            user: answers.get(&i.id.to_string()).cloned().unwrap_or_default(),
        })
        .collect();

    let results = ai::grade(&cfg, &state.http, &inputs).await;

    let mut ok = 0i64;
    let mut partial = 0i64;
    let mut wrong = 0i64;
    for r in &results {
        match r.verdict.as_str() {
            "ok" => ok += 1,
            "partial" => partial += 1,
            _ => wrong += 1,
        }
    }

    let pairs: Vec<(i64, String)> = results.iter().map(|r| (r.id, r.verdict.clone())).collect();
    let _ = state.store.apply_results(&pairs);

    let results_json = serde_json::to_value(&results).unwrap_or(json!([]));
    let _ = state
        .store
        .finish_session(session_id, &results_json, ok, partial, wrong);

    let detail: Vec<Value> = items
        .iter()
        .map(|i| {
            let r = results.iter().find(|x| x.id == i.id);
            json!({
                "id": i.id,
                "prompt": i.prompt,
                "key": i.key,
                "user": answers.get(&i.id.to_string()).cloned().unwrap_or_default(),
                "verdict": r.map(|x| x.verdict.clone()).unwrap_or_else(|| "partial".into()),
                "reason": r.map(|x| x.reason.clone()).unwrap_or_default(),
            })
        })
        .collect();

    let due = rules::mixed_due(&state.store, &cfg);
    let payload = json!({
        "kind": kind,
        "score": { "ok": ok, "partial": partial, "wrong": wrong },
        "detail": detail,
        "offline": results.iter().any(|r| r.offline),
        "nextMixedDue": { "due": due.due, "reason": due.reason }
    });

    if let Ok(mut g) = state.result.lock() {
        *g = Some(payload);
    }
    if let Ok(mut g) = state.session.lock() {
        *g = None;
    }

    open_aux(app, "result", "result.html", 820.0, 660.0)?;
    if let Some(w) = app.get_webview_window("quiz") {
        let _ = w.close();
    }
    Ok(())
}

/* ============================================================
   定时
   ============================================================ */

fn tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    let cfg = cfg_of(&state);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    if state.store.get_meta("last_archive").as_deref() != Some(today.as_str()) {
        let _ = state.store.archive_stale(cfg.pending_days);
        let _ = state.store.set_meta("last_archive", &today);
        let _ = refresh_tray(app);
    }

    let key = format!("notified_{}", today);
    if state.store.get_meta(&key).is_some() {
        return;
    }

    let parts: Vec<i64> = cfg
        .quiz_time
        .split(':')
        .filter_map(|p| p.parse::<i64>().ok())
        .collect();
    let h = parts.first().copied().unwrap_or(21).clamp(0, 23) as u32;
    let m = parts.get(1).copied().unwrap_or(0).clamp(0, 59) as u32;

    let now = chrono::Local::now();
    let Some(target) = now
        .date_naive()
        .and_hms_opt(h, m, 0)
        .and_then(|n| n.and_local_timezone(chrono::Local).single())
    else {
        return;
    };
    if now < target {
        return;
    }

    if let Ok(p) = rules::pick_session(&state.store, &cfg) {
        let _ = state.store.set_meta(&key, "1");
        if p.kind.is_some() {
            notify(app, "回问 · 今天的卷子", &format!("{} 道题，点开开考", p.rows.len()));
        }
    }
}

/* ============================================================
   命令
   ============================================================ */

#[tauri::command]
fn capture_init(app: AppHandle, state: tauri::State<'_, AppState>) -> Value {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let cfg = cfg_of(&state);

    let pending: Vec<Value> = state
        .store
        .list_pending(cfg.pending_days)
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "question": r.question,
                "source": r.source,
                "created_at": r.created_at
            })
        })
        .collect();

    let clip = app.clipboard().read_text().unwrap_or_default();
    let clip: String = clip.chars().take(4000).collect();

    json!({
        "pending": pending,
        "clipboard": clip,
        "hotkey": cfg.hotkey,
        "todayAnswered": state.store.list_today(cfg.daily_limit).map(|v| v.len()).unwrap_or(0)
    })
}

#[tauri::command]
fn capture_foreground(state: tauri::State<'_, AppState>) -> String {
    let v = state
        .last_fg
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    v
}

#[tauri::command]
fn renderer_ready(info: Value) {
    eprintln!("[renderer] {}", info);
}

#[derive(Deserialize)]
struct SavePayload {
    question: String,
    #[serde(default)]
    answer: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    context: String,
}

#[tauri::command]
fn capture_save(state: tauri::State<'_, AppState>, app: AppHandle, payload: SavePayload) -> Value {
    if payload.question.trim().is_empty() {
        return json!({ "ok": false, "error": "问题不能为空" });
    }
    match state.store.add_question(
        &payload.question,
        Some(payload.answer.as_str()),
        Some(payload.source.as_str()),
        Some(payload.context.as_str()),
    ) {
        Ok(id) => {
            let _ = refresh_tray(&app);
            json!({ "ok": true, "id": id })
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

#[tauri::command]
fn refill_save(state: tauri::State<'_, AppState>, app: AppHandle, id: i64, answer: String) -> Value {
    match state.store.set_answer(id, &answer) {
        Ok(_) => {
            let _ = refresh_tray(&app);
            json!({ "ok": true })
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

#[tauri::command]
fn capture_resize(app: AppHandle, height: f64) -> Value {
    if let Some(w) = app.get_webview_window("capture") {
        let h = if height >= CAP_H_OPEN - 1.0 {
            CAP_H_OPEN
        } else {
            CAP_H
        };
        let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(CAP_W, h)));
    }
    json!({ "ok": true })
}

#[tauri::command]
fn capture_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window("capture") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn capture_open_history(app: AppHandle) {
    if let Some(w) = app.get_webview_window("capture") {
        let _ = w.hide();
    }
    let _ = open_history(&app);
}

#[tauri::command]
fn history_list(
    state: tauri::State<'_, AppState>,
    scope: Option<String>,
    keyword: Option<String>,
) -> Value {
    let rows = state
        .store
        .search(
            scope.as_deref().unwrap_or("all"),
            keyword.as_deref().unwrap_or(""),
            300,
        )
        .unwrap_or_default();
    json!({
        "rows": serde_json::to_value(&rows).unwrap_or(json!([])),
        "counts": state.store.scope_counts()
    })
}

#[tauri::command]
fn quiz_get(state: tauri::State<'_, AppState>) -> Value {
    let guard = match state.session.lock() {
        Ok(g) => g,
        Err(_) => return Value::Null,
    };
    match guard.as_ref() {
        Some(s) => json!({
            "kind": s.kind,
            "items": serde_json::to_value(&s.items).unwrap_or(json!([]))
        }),
        None => Value::Null,
    }
}

#[tauri::command]
async fn quiz_start(app: AppHandle, kind: Option<String>) {
    start_quiz(app, kind).await;
}

#[tauri::command]
async fn quiz_submit(
    app: AppHandle,
    answers: std::collections::HashMap<String, String>,
) -> Value {
    match submit_quiz(&app, answers).await {
        Ok(_) => json!({ "ok": true }),
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

#[tauri::command]
fn result_get(state: tauri::State<'_, AppState>) -> Value {
    state
        .result
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or(Value::Null)
}

#[tauri::command]
fn settings_get(state: tauri::State<'_, AppState>) -> Value {
    let cfg = cfg_of(&state);
    let mut v = serde_json::to_value(&cfg).unwrap_or(json!({}));
    if let Value::Object(ref mut m) = v {
        m.insert("stats".into(), state.store.stats());
    }
    v
}

#[tauri::command]
fn settings_set(app: AppHandle, state: tauri::State<'_, AppState>, patch: Value) -> Value {
    let mut guard = match state.settings.lock() {
        Ok(g) => g,
        Err(e) => return json!({ "ok": false, "error": e.to_string() }),
    };

    let mut current = serde_json::to_value(&*guard).unwrap_or(json!({}));
    if let (Value::Object(cur), Value::Object(p)) = (&mut current, &patch) {
        for (k, v) in p {
            cur.insert(k.clone(), v.clone());
        }
    }
    let parsed: Settings = serde_json::from_value(current).unwrap_or_default();
    let hotkey_changed = parsed.hotkey != guard.hotkey;
    parsed.save(&state.data_dir);
    *guard = parsed.clone();
    drop(guard);

    if hotkey_changed {
        if let Err(e) = register_hotkey(&app, &parsed) {
            notify(&app, "热键没注册上", &e);
        }
    }
    let _ = refresh_tray(&app);
    json!({ "ok": true })
}

#[tauri::command]
fn app_open_data_dir(state: tauri::State<'_, AppState>) -> Value {
    match open_path(&state.data_dir) {
        Ok(_) => json!({ "ok": true }),
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

#[tauri::command]
fn app_backup(state: tauri::State<'_, AppState>) -> Value {
    match state.store.backup() {
        Ok(f) => json!({ "ok": true, "file": f.to_string_lossy() }),
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

#[tauri::command]
fn app_quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn win_close(window: tauri::Window) {
    let _ = window.close();
}

/* ============================================================
   启动
   ============================================================ */

fn print_banner(cfg: &Settings, dir: &std::path::Path, first: bool) {
    let hot = cfg.hotkey.replace("Control", "Ctrl").replace('+', " + ");
    let theme = if first { "（首次运行）" } else { "" };
    let rule = format!("  {}", "─".repeat(44));
    println!();
    println!("  回问 Reask 已启动{}", theme);
    println!("{}", rule);
    println!("  记问题    {}", hot);
    println!("  托盘      右下角青色圆环，可能收在 ^ 里");
    println!("  数据      {}", dir.display());
    println!("{}", rule);
    println!("  这个窗口会一直被占着，正常。关掉它 = 退出应用。");
    println!("  想让它后台待着，把这个窗口最小化就行。");
    println!();
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let _ = show_capture(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();

            let data_dir = handle
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            std::fs::create_dir_all(&data_dir).ok();

            let store = Store::open(&data_dir)
                .map_err(|e| format!("打开数据库失败：{}", e))?;
            let cfg = Settings::load(&data_dir);

            store.archive_stale(cfg.pending_days).ok();
            if cfg.auto_backup {
                if let Err(e) = store.backup() {
                    eprintln!("[backup] 失败 {}", e);
                }
            }

            let first_run = store.get_meta("first_run").is_none();
            if first_run {
                let _ = store.set_meta("first_run", &store::now_iso());
            }

            app.manage(AppState {
                store,
                settings: Mutex::new(cfg.clone()),
                http: reqwest::Client::new(),
                session: Mutex::new(None),
                result: Mutex::new(None),
                data_dir: data_dir.clone(),
                last_fg: Mutex::new(String::new()),
                first_show: Mutex::new(true),
            });

            ensure_capture(&handle).ok();
            build_tray(&handle).ok();
            refresh_tray(&handle).ok();

            if let Err(e) = register_hotkey(&handle, &cfg) {
                eprintln!("[hotkey] {}", e);
                notify(&handle, "热键被占用", &e);
            }

            print_banner(&cfg, &data_dir, first_run);

            if first_run {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1200));
                    let _ = show_capture(&h);
                    notify(&h, "回问已经跑起来了", "平时按热键记问题。图标在托盘，可能收在 ^ 里。");
                });
            }

            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(8)).await;
                loop {
                    let h2 = h.clone();
                    let _ = tokio::task::spawn_blocking(move || tick(&h2)).await;
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            capture_init,
            capture_foreground,
            renderer_ready,
            capture_save,
            refill_save,
            capture_resize,
            capture_hide,
            capture_open_history,
            history_list,
            quiz_get,
            quiz_start,
            quiz_submit,
            result_get,
            settings_get,
            settings_set,
            app_open_data_dir,
            app_backup,
            app_quit,
            win_close
        ])
        .run(tauri::generate_context!())
        .expect("回问启动失败");
}

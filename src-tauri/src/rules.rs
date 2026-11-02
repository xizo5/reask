use crate::settings::Settings;
use crate::store::{QuestionRow, Store};
use chrono::{DateTime, Datelike, Local};

pub fn local_day(d: &DateTime<Local>) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

fn parse_iso(s: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Local))
}

pub fn daily_done_today(store: &Store) -> bool {
    match store.last_finished_session("daily") {
        Some((started, _)) => match parse_iso(&started) {
            Some(d) => local_day(&d) == local_day(&Local::now()),
            None => false,
        },
        None => false,
    }
}

/// 距上次大杂烩多少天，None 表示从没做过
pub fn days_since_mixed(store: &Store) -> Option<f64> {
    let (_, finished) = store.last_finished_session("mixed")?;
    let d = parse_iso(&finished)?;
    let secs = Local::now().signed_duration_since(d).num_seconds() as f64;
    Some(secs / 86400.0)
}

pub struct Due {
    pub due: bool,
    pub reason: String,
}

pub fn mixed_due(store: &Store, cfg: &Settings) -> Due {
    match days_since_mixed(store) {
        None => Due {
            due: true,
            reason: "还没做过大杂烩".into(),
        },
        Some(d) => {
            if d >= cfg.mixed_interval_days as f64 {
                Due {
                    due: true,
                    reason: format!("距上次 {} 天", d.floor() as i64),
                }
            } else {
                Due {
                    due: false,
                    reason: format!(
                        "还要等 {} 天",
                        (cfg.mixed_interval_days as f64 - d).ceil() as i64
                    ),
                }
            }
        }
    }
}

pub struct Picked {
    pub kind: Option<String>,
    pub reason: String,
    pub rows: Vec<QuestionRow>,
}

/// 决定这次开哪种卷
pub fn pick_session(store: &Store, cfg: &Settings) -> Result<Picked, String> {
    let today = store.list_today(cfg.daily_limit)?;
    if !today.is_empty() && !daily_done_today(store) {
        return Ok(Picked {
            kind: Some("daily".into()),
            reason: "今天记下的问题".into(),
            rows: today,
        });
    }

    let due = mixed_due(store, cfg);
    if due.due {
        let pool = store.list_mixed(cfg.mixed_limit)?;
        if !pool.is_empty() {
            return Ok(Picked {
                kind: Some("mixed".into()),
                reason: due.reason,
                rows: pool,
            });
        }
    }

    let reason = if today.is_empty() {
        "今天还没记下有答案的问题".to_string()
    } else {
        "今天的卷子已经做完了".to_string()
    };
    Ok(Picked {
        kind: None,
        reason,
        rows: vec![],
    })
}

pub fn rows_for(store: &Store, cfg: &Settings, kind: &str) -> Result<Vec<QuestionRow>, String> {
    match kind {
        "daily" => store.list_today(cfg.daily_limit),
        _ => store.list_mixed(cfg.mixed_limit),
    }
}

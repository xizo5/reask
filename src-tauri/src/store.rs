use rusqlite::{params_from_iter, types::Value, Connection, Row};
use serde::Serialize;
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS questions(
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  question    TEXT NOT NULL,
  answer      TEXT,
  source      TEXT,
  context     TEXT,
  created_at  TEXT NOT NULL,
  answered_at TEXT,
  last_asked_at TEXT,
  ask_count   INTEGER NOT NULL DEFAULT 0,
  ok_count    INTEGER NOT NULL DEFAULT 0,
  wrong_count INTEGER NOT NULL DEFAULT 0,
  archived    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_q_created ON questions(created_at);
CREATE INDEX IF NOT EXISTS idx_q_arch    ON questions(archived);

CREATE TABLE IF NOT EXISTS sessions(
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  kind       TEXT NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  payload    TEXT,
  results    TEXT,
  ok         INTEGER DEFAULT 0,
  partial    INTEGER DEFAULT 0,
  wrong      INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT);
"#;

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Serialize, Clone, Debug)]
pub struct QuestionRow {
    pub id: i64,
    pub question: String,
    pub answer: Option<String>,
    pub source: Option<String>,
    pub context: Option<String>,
    pub created_at: String,
    pub answered_at: Option<String>,
    pub last_asked_at: Option<String>,
    pub ask_count: i64,
    pub ok_count: i64,
    pub wrong_count: i64,
    pub archived: i64,
}

fn map_row(r: &Row) -> rusqlite::Result<QuestionRow> {
    Ok(QuestionRow {
        id: r.get("id")?,
        question: r.get("question")?,
        answer: r.get("answer")?,
        source: r.get("source")?,
        context: r.get("context")?,
        created_at: r.get("created_at")?,
        answered_at: r.get("answered_at")?,
        last_asked_at: r.get("last_asked_at")?,
        ask_count: r.get("ask_count")?,
        ok_count: r.get("ok_count")?,
        wrong_count: r.get("wrong_count")?,
        archived: r.get("archived")?,
    })
}

pub struct Store {
    conn: Mutex<Connection>,
    pub dir: PathBuf,
    pub file: PathBuf,
}

impl Store {
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let file = dir.join("reask.db");
        let conn = Connection::open(&file).map_err(|e| e.to_string())?;
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(Self {
            conn: Mutex::new(conn),
            dir: dir.to_path_buf(),
            file,
        })
    }

    fn c(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn query(&self, sql: &str, vals: Vec<Value>) -> Result<Vec<QuestionRow>, String> {
        let conn = self.c();
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params_from_iter(vals.iter()), map_row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    }

    fn scalar(&self, sql: &str, vals: Vec<Value>) -> Result<i64, String> {
        let conn = self.c();
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let v: Option<i64> = stmt
            .query_row(params_from_iter(vals.iter()), |r| r.get::<_, Option<i64>>(0))
            .map_err(|e| e.to_string())?;
        Ok(v.unwrap_or(0))
    }

    /* ---------- meta ---------- */

    pub fn get_meta(&self, key: &str) -> Option<String> {
        let conn = self.c();
        conn.query_row("SELECT value FROM meta WHERE key=?", [key], |r| r.get::<_, String>(0))
            .ok()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.c();
        conn.execute(
            "INSERT INTO meta(key,value) VALUES(?,?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /* ---------- 写入 ---------- */

    pub fn add_question(
        &self,
        question: &str,
        answer: Option<&str>,
        source: Option<&str>,
        context: Option<&str>,
    ) -> Result<i64, String> {
        let now = now_iso();
        let ans = answer.map(|a| a.trim()).filter(|a| !a.is_empty());
        let conn = self.c();
        conn.execute(
            "INSERT INTO questions(question,answer,source,context,created_at,answered_at)
             VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![
                question.trim(),
                ans,
                source.filter(|s| !s.is_empty()),
                context.filter(|s| !s.is_empty()),
                now,
                ans.map(|_| now.clone())
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn set_answer(&self, id: i64, answer: &str) -> Result<(), String> {
        let conn = self.c();
        conn.execute(
            "UPDATE questions SET answer=?, answered_at=? WHERE id=?",
            rusqlite::params![answer.trim(), now_iso(), id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /* ---------- 查询 ---------- */

    pub fn list_pending(&self, days: i64) -> Result<Vec<QuestionRow>, String> {
        self.query(
            "SELECT * FROM questions
              WHERE archived=0
                AND (answer IS NULL OR TRIM(answer)='')
                AND datetime(created_at) >= datetime('now', ?)
              ORDER BY created_at DESC",
            vec![Value::Text(format!("-{} days", days))],
        )
    }

    pub fn count_pending(&self, days: i64) -> i64 {
        self.scalar(
            "SELECT COUNT(*) FROM questions
              WHERE archived=0
                AND (answer IS NULL OR TRIM(answer)='')
                AND datetime(created_at) >= datetime('now', ?)",
            vec![Value::Text(format!("-{} days", days))],
        )
        .unwrap_or(0)
    }

    pub fn archive_stale(&self, days: i64) -> Result<(), String> {
        let conn = self.c();
        conn.execute(
            "UPDATE questions SET archived=1
              WHERE archived=0
                AND (answer IS NULL OR TRIM(answer)='')
                AND datetime(created_at) < datetime('now', ?)",
            [format!("-{} days", days)],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_today(&self, limit: i64) -> Result<Vec<QuestionRow>, String> {
        self.query(
            "SELECT * FROM questions
              WHERE archived=0
                AND answer IS NOT NULL AND TRIM(answer)<>''
                AND date(created_at,'localtime') = date('now','localtime')
              ORDER BY created_at ASC LIMIT ?",
            vec![Value::Integer(limit)],
        )
    }

    pub fn list_mixed(&self, limit: i64) -> Result<Vec<QuestionRow>, String> {
        self.query(
            "SELECT * FROM questions
              WHERE archived=0 AND answer IS NOT NULL AND TRIM(answer)<>''
              ORDER BY CASE WHEN wrong_count > ok_count THEN 0 ELSE 1 END, RANDOM()
              LIMIT ?",
            vec![Value::Integer(limit)],
        )
    }

    pub fn mark_asked(&self, ids: &[i64]) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        let now = now_iso();
        let ph = vec!["?"; ids.len()].join(",");
        let sql = format!(
            "UPDATE questions SET last_asked_at=?, ask_count=ask_count+1 WHERE id IN ({})",
            ph
        );
        let mut vals: Vec<Value> = vec![Value::Text(now)];
        vals.extend(ids.iter().map(|i| Value::Integer(*i)));
        let conn = self.c();
        conn.execute(&sql, params_from_iter(vals.iter()))
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn apply_results(&self, results: &[(i64, String)]) -> Result<(), String> {
        let mut conn = self.c();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for (id, verdict) in results {
            if verdict == "ok" {
                tx.execute("UPDATE questions SET ok_count=ok_count+1 WHERE id=?", [id])
                    .map_err(|e| e.to_string())?;
            } else {
                tx.execute("UPDATE questions SET wrong_count=wrong_count+1 WHERE id=?", [id])
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /* ---------- 浏览 ---------- */

    pub fn search(&self, scope: &str, keyword: &str, limit: i64) -> Result<Vec<QuestionRow>, String> {
        let mut where_parts: Vec<String> = vec![];
        let mut vals: Vec<Value> = vec![];

        match scope {
            "today" => where_parts.push("date(created_at,'localtime') = date('now','localtime')".into()),
            "pending" => where_parts
                .push("archived=0 AND (answer IS NULL OR TRIM(answer)='')".into()),
            "answered" => where_parts
                .push("archived=0 AND answer IS NOT NULL AND TRIM(answer)<>''".into()),
            "archived" => where_parts.push("archived=1".into()),
            _ => {}
        }

        let kw = keyword.trim();
        if !kw.is_empty() {
            where_parts.push("(question LIKE ? OR answer LIKE ? OR source LIKE ?)".into());
            let like = format!("%{}%", kw);
            vals.push(Value::Text(like.clone()));
            vals.push(Value::Text(like.clone()));
            vals.push(Value::Text(like));
        }

        let where_sql = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };
        vals.push(Value::Integer(limit));

        self.query(
            &format!(
                "SELECT * FROM questions {} ORDER BY created_at DESC LIMIT ?",
                where_sql
            ),
            vals,
        )
    }

    pub fn scope_counts(&self) -> Json {
        let conn = self.c();
        let r: Option<(i64, i64, i64, i64, i64)> = conn
            .query_row(
                "SELECT
                   COALESCE(SUM(CASE WHEN date(created_at,'localtime')=date('now','localtime') THEN 1 ELSE 0 END),0),
                   COALESCE(SUM(CASE WHEN archived=0 THEN 1 ELSE 0 END),0),
                   COALESCE(SUM(CASE WHEN archived=0 AND (answer IS NULL OR TRIM(answer)='') THEN 1 ELSE 0 END),0),
                   COALESCE(SUM(CASE WHEN archived=1 THEN 1 ELSE 0 END),0),
                   COUNT(*)
                 FROM questions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .ok();
        let (today, alive, pending, archived, total) = r.unwrap_or((0, 0, 0, 0, 0));
        json!({
            "today": today,
            "all": total,
            "alive": alive,
            "pending": pending,
            "archived": archived
        })
    }

    /* ---------- 场次 ---------- */

    pub fn create_session(&self, kind: &str, payload: &Json) -> Result<i64, String> {
        let conn = self.c();
        conn.execute(
            "INSERT INTO sessions(kind,started_at,payload) VALUES(?,?,?)",
            rusqlite::params![kind, now_iso(), payload.to_string()],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn finish_session(
        &self,
        id: i64,
        results: &Json,
        ok: i64,
        partial: i64,
        wrong: i64,
    ) -> Result<(), String> {
        let conn = self.c();
        conn.execute(
            "UPDATE sessions SET finished_at=?, results=?, ok=?, partial=?, wrong=? WHERE id=?",
            rusqlite::params![now_iso(), results.to_string(), ok, partial, wrong, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn last_finished_session(&self, kind: &str) -> Option<(String, String)> {
        let conn = self.c();
        conn.query_row(
            "SELECT started_at, finished_at FROM sessions
              WHERE kind=? AND finished_at IS NOT NULL ORDER BY id DESC LIMIT 1",
            [kind],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .ok()
    }

    /* ---------- 统计 / 备份 ---------- */

    pub fn stats(&self) -> Json {
        let conn = self.c();
        let (total, answered): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN answer IS NOT NULL AND TRIM(answer)<>'' THEN 1 ELSE 0 END),0)
                   FROM questions WHERE archived=0",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0));

        let (sessions, ok, wrong): (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(ok),0), COALESCE(SUM(wrong),0)
                   FROM sessions WHERE finished_at IS NOT NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap_or((0, 0, 0));

        let decided = ok + wrong;
        json!({
            "total": total,
            "answered": answered,
            "sessions": sessions,
            "rate": if decided > 0 { json!(ok * 100 / decided) } else { Json::Null }
        })
    }

    fn dump_table(&self, table: &str) -> Json {
        let conn = self.c();
        let sql = format!("SELECT * FROM {} ORDER BY id", table);
        let Ok(mut stmt) = conn.prepare(&sql) else {
            return json!([]);
        };
        let cols: Vec<String> = stmt.column_names().map(|s| s.to_string()).collect();
        let n = cols.len();
        let mut out = vec![];
        if let Ok(rows) = stmt.query_map([], |r| {
            let mut obj = serde_json::Map::new();
            for i in 0..n {
                let v: Value = r.get(i)?;
                obj.insert(
                    cols[i].clone(),
                    match v {
                        Value::Null => Json::Null,
                        Value::Integer(i) => json!(i),
                        Value::Real(f) => json!(f),
                        Value::Text(s) => json!(s),
                        Value::Blob(b) => json!(b.len()),
                    },
                );
            }
            Ok(Json::Object(obj))
        }) {
            for row in rows.flatten() {
                out.push(row);
            }
        }
        Json::Array(out)
    }

    pub fn export_json(&self) -> Json {
        json!({
            "exportedAt": now_iso(),
            "questions": self.dump_table("questions"),
            "sessions": self.dump_table("sessions"),
            "meta": self.dump_table("meta"),
        })
    }

    pub fn backup(&self) -> Result<PathBuf, String> {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let dir = self.dir.join("backup");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let f = dir.join(format!("reask-{}.json", day));
        if f.exists() {
            return Ok(f);
        }
        let body = serde_json::to_string_pretty(&self.export_json()).map_err(|e| e.to_string())?;
        std::fs::write(&f, body).map_err(|e| e.to_string())?;
        Ok(f)
    }
}

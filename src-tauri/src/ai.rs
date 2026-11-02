use crate::settings::Settings;
use crate::store::QuestionRow;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct QuizItem {
    pub id: i64,
    pub prompt: String,
    pub key: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GradeInput {
    pub id: i64,
    pub prompt: String,
    pub key: String,
    pub user: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GradeResult {
    pub id: i64,
    pub verdict: String,
    pub reason: String,
    pub offline: bool,
}

fn configured(cfg: &Settings) -> bool {
    !cfg.api_key.trim().is_empty()
}

async fn chat(
    cfg: &Settings,
    client: &reqwest::Client,
    system: &str,
    user: &str,
) -> Result<Value, String> {
    let base = cfg.base_url.trim_end_matches('/').to_string();
    let body = json!({
        "model": cfg.model,
        "temperature": 0.2,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    });

    let res = client
        .post(format!("{}/chat/completions", base))
        .bearer_auth(cfg.api_key.trim())
        .json(&body)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| format!("网络错误：{}", e))?;

    let status = res.status();
    let text = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("AI 接口 {}：{}", status.as_u16(), truncate(&text, 200)));
    }

    let payload: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let content = payload["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("AI 返回为空")?;
    serde_json::from_str(content).map_err(|e| format!("解析 AI 输出失败：{}", e))
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn fallback_item(r: &QuestionRow) -> QuizItem {
    QuizItem {
        id: r.id,
        prompt: r.question.clone(),
        key: truncate(r.answer.as_deref().unwrap_or(""), 600),
    }
}

pub async fn build_quiz(
    cfg: &Settings,
    client: &reqwest::Client,
    rows: &[QuestionRow],
) -> Vec<QuizItem> {
    if rows.is_empty() {
        return vec![];
    }
    if !configured(cfg) {
        return rows.iter().map(fallback_item).collect();
    }

    let input: Vec<Value> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "id": i + 1,
                "question": r.question,
                "answer": truncate(r.answer.as_deref().unwrap_or(""), 800)
            })
        })
        .collect();

    let system = "你是出题助手，负责把用户记下的「问题-答案」改写成可用于检验记忆的题目。只输出 JSON，不要任何解释。";
    let user = format!(
        "下面是 {} 条记录：\n{}\n\n\
         为每一条生成一道 30 秒内能答完的题，要求：\n\
         1. 题干必须可判定对错，问原因、区别、做法或结论，不要问「你还记得吗」。\n\
         2. 题干里绝对不能出现答案。\n\
         3. 答案很长时只取最关键的一个点出题。\n\
         4. 中文，题干不超过 40 个字。\n\
         5. key 是标准答案要点，一到两句，用于判分对照。\n\n\
         输出格式：{{\"items\":[{{\"id\":1,\"prompt\":\"题干\",\"key\":\"要点\"}}]}}",
        input.len(),
        serde_json::to_string(&input).unwrap_or_default()
    );

    match chat(cfg, client, system, &user).await {
        Ok(out) => {
            let mut result: Vec<QuizItem> = vec![];
            if let Some(items) = out["items"].as_array() {
                for it in items {
                    let idx = it["id"].as_i64().unwrap_or(0);
                    if idx < 1 || idx as usize > rows.len() {
                        continue;
                    }
                    let row = &rows[(idx - 1) as usize];
                    if result.iter().any(|x| x.id == row.id) {
                        continue;
                    }
                    result.push(QuizItem {
                        id: row.id,
                        prompt: truncate(it["prompt"].as_str().unwrap_or(&row.question), 200),
                        key: truncate(
                            it["key"].as_str().unwrap_or(row.answer.as_deref().unwrap_or("")),
                            600,
                        ),
                    });
                }
            }
            // AI 漏掉的条目用原题兜底，不允许丢题
            for r in rows {
                if !result.iter().any(|x| x.id == r.id) {
                    result.push(fallback_item(r));
                }
            }
            result
        }
        Err(e) => {
            eprintln!("[ai] 出题失败，降级为原题：{}", e);
            rows.iter().map(fallback_item).collect()
        }
    }
}

pub async fn grade(
    cfg: &Settings,
    client: &reqwest::Client,
    items: &[GradeInput],
) -> Vec<GradeResult> {
    if items.is_empty() {
        return vec![];
    }
    if !configured(cfg) {
        return items.iter().map(fallback_grade).collect();
    }

    let input: Vec<Value> = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            json!({
                "id": i + 1,
                "question": it.prompt,
                "key": it.key,
                "user": if it.user.trim().is_empty() { "（空白，没有作答）".to_string() } else { it.user.clone() }
            })
        })
        .collect();

    let system = "你是严格的判分员。判断用户的回答是否答出了要点。只输出 JSON。";
    let user = format!(
        "题目与作答：\n{}\n\n\
         判定标准：\n\
         - ok：核心结论正确。措辞不同、表达粗糙都不算错。\n\
         - partial：方向对，但缺了关键信息，或只说了一半。\n\
         - wrong：核心结论错误、答非所问、或基本空白。\n\
         不要因为不完整就一律判 wrong。\n\n\
         输出格式：{{\"results\":[{{\"id\":1,\"verdict\":\"ok\",\"reason\":\"一句话说明差在哪\"}}]}}",
        serde_json::to_string(&input).unwrap_or_default()
    );

    match chat(cfg, client, system, &user).await {
        Ok(out) => {
            let mut result: Vec<GradeResult> = vec![];
            if let Some(list) = out["results"].as_array() {
                for r in list {
                    let idx = r["id"].as_i64().unwrap_or(0);
                    if idx < 1 || idx as usize > items.len() {
                        continue;
                    }
                    let it = &items[(idx - 1) as usize];
                    if result.iter().any(|x| x.id == it.id) {
                        continue;
                    }
                    let raw = r["verdict"].as_str().unwrap_or("partial");
                    let verdict = match raw {
                        "ok" | "partial" | "wrong" => raw.to_string(),
                        _ => "partial".to_string(),
                    };
                    result.push(GradeResult {
                        id: it.id,
                        verdict,
                        reason: truncate(r["reason"].as_str().unwrap_or(""), 200),
                        offline: false,
                    });
                }
            }
            for it in items {
                if !result.iter().any(|x| x.id == it.id) {
                    let mut fb = fallback_grade(it);
                    fb.offline = true;
                    result.push(fb);
                }
            }
            result
        }
        Err(e) => {
            eprintln!("[ai] 判分失败，降级为关键词比对：{}", e);
            items.iter().map(fallback_grade).collect()
        }
    }
}

/* ---------- 离线降级 ---------- */

fn fallback_grade(it: &GradeInput) -> GradeResult {
    let ratio = overlap(&it.key, &it.user);
    let verdict = if ratio >= 0.45 {
        "ok"
    } else if ratio >= 0.2 {
        "partial"
    } else {
        "wrong"
    };
    GradeResult {
        id: it.id,
        verdict: verdict.into(),
        reason: format!("离线判分：与标准答案关键词重合 {}%", (ratio * 100.0).round() as i64),
        offline: true,
    }
}

fn overlap(key: &str, user: &str) -> f64 {
    let k = tokens(key);
    if k.is_empty() {
        return 0.0;
    }
    let u: std::collections::HashSet<String> = tokens(user).into_iter().collect();
    if u.is_empty() {
        return 0.0;
    }
    let hit = k.iter().filter(|t| u.contains(*t)).count();
    hit as f64 / k.len() as f64
}

fn tokens(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let cjk: Vec<char> = lower
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fa5}').contains(c))
        .collect();

    let mut out: Vec<String> = Vec::new();
    for w in cjk.windows(2) {
        out.push(w.iter().collect());
    }

    let mut cur = String::new();
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            cur.push(ch);
        } else {
            if cur.chars().count() >= 2 {
                out.push(cur.clone());
            }
            cur.clear();
        }
    }
    if cur.chars().count() >= 2 {
        out.push(cur);
    }

    out.sort();
    out.dedup();
    out
}

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub hotkey: String,
    pub quiz_time: String,
    pub mixed_interval_days: i64,
    pub mixed_limit: i64,
    pub daily_limit: i64,
    pub pending_days: i64,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub auto_backup: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Control+Shift+Q".into(),
            quiz_time: "21:00".into(),
            mixed_interval_days: 3,
            mixed_limit: 20,
            daily_limit: 30,
            pending_days: 7,
            api_key: String::new(),
            base_url: "https://api.deepseek.com/v1".into(),
            model: "deepseek-chat".into(),
            auto_backup: true,
        }
    }
}

fn path_of(dir: &Path) -> PathBuf {
    dir.join("settings.json")
}

impl Settings {
    pub fn load(dir: &Path) -> Self {
        let p = path_of(dir);
        match std::fs::read_to_string(&p) {
            Ok(raw) => serde_json::from_str::<Settings>(&raw).unwrap_or_default(),
            Err(_) => {
                let s = Settings::default();
                s.save(dir);
                s
            }
        }
    }

    pub fn save(&self, dir: &Path) {
        if std::fs::create_dir_all(dir).is_ok() {
            if let Ok(body) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path_of(dir), body);
            }
        }
    }
}

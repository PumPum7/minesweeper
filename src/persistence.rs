use web_sys::Storage;

const DIFFICULTY_KEY: &str = "ms.difficulty";
const THEME_KEY: &str = "ms.theme";

fn storage() -> Option<Storage> {
    let window = web_sys::window()?;
    window.local_storage().ok().flatten()
}

pub fn load_difficulty() -> Option<String> {
    storage()?.get_item(DIFFICULTY_KEY).ok().flatten()
}

pub fn save_difficulty(value: &str) {
    if let Some(store) = storage() {
        let _ = store.set_item(DIFFICULTY_KEY, value);
    }
}

pub fn load_best_time_seconds(difficulty_key: &str) -> Option<u64> {
    let key = format!("ms.best.{difficulty_key}");
    let raw = storage()?.get_item(&key).ok().flatten()?;
    raw.parse::<u64>().ok()
}

pub fn save_best_time_seconds(difficulty_key: &str, seconds: u64) {
    let key = format!("ms.best.{difficulty_key}");
    if let Some(store) = storage() {
        let _ = store.set_item(&key, &seconds.to_string());
    }
}

pub fn load_theme() -> Option<String> {
    storage()?.get_item(THEME_KEY).ok().flatten()
}

pub fn save_theme(value: &str) {
    if let Some(store) = storage() {
        let _ = store.set_item(THEME_KEY, value);
    }
}

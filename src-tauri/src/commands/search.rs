use std::time::Instant;

use tauri::State;

use crate::database::models::Application;
use crate::state::AppState;

#[tauri::command]
pub fn search_applications(keyword: String, state: State<AppState>) -> Result<Vec<Application>, String> {
    let started = Instant::now();
    let index = state.lock_index()?;
    let results = index.search(&keyword);
    let elapsed = started.elapsed();
    if elapsed.as_millis() >= 20 {
        log::warn!("搜索 {}ms keyword={keyword}", elapsed.as_millis());
    } else if elapsed.as_millis() >= 5 {
        log::debug!("搜索 {}ms keyword={keyword}", elapsed.as_millis());
    }
    Ok(results)
}

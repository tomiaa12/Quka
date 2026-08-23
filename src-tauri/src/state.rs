use std::sync::Mutex;

use rusqlite::Connection;

use crate::database::applications::query_all;
use crate::database::search::SearchIndex;
use crate::database::settings;

#[derive(Debug)]
pub enum AppError {
    Database(String),
    Scanner(String),
    Launcher(String),
    Shortcut(String),
    Io(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(message) => write!(f, "数据库异常：{message}"),
            Self::Scanner(message) => write!(f, "{message}"),
            Self::Launcher(message) => write!(f, "{message}"),
            Self::Shortcut(message) => write!(f, "{message}"),
            Self::Io(message) => write!(f, "{message}"),
        }
    }
}

impl From<AppError> for String {
    fn from(value: AppError) -> Self {
        value.to_string()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value.to_string())
    }
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub index: Mutex<SearchIndex>,
    pub just_initialized: bool,
}

impl AppState {
    pub fn lock_db(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.db
            .lock()
            .map_err(|error| AppError::Database(error.to_string()))
    }

    pub fn lock_index(&self) -> Result<std::sync::MutexGuard<'_, SearchIndex>, AppError> {
        self.index
            .lock()
            .map_err(|error| AppError::Database(error.to_string()))
    }

    pub fn refresh_index(&self) -> Result<(), AppError> {
        let (apps, settings) = {
            let conn = self.lock_db()?;
            (query_all(&conn)?, settings::get(&conn)?)
        };
        let mut index = self.lock_index()?;
        *index = SearchIndex::build(apps, settings.result_limit, settings.enable_usage_ranking);
        Ok(())
    }
}

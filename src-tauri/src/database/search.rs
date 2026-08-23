use pinyin::ToPinyin;

use super::models::Application;

fn words(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|char: char| char.is_whitespace() || matches!(char, '-' | '_' | '.'))
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn is_subsequence(text: &str, keyword: &str) -> bool {
    let mut chars = keyword.chars();
    let mut current = chars.next();
    for item in text.chars() {
        if Some(item) == current {
            current = chars.next();
            if current.is_none() {
                return true;
            }
        }
    }
    current.is_none()
}

fn pinyin_keys(name: &str) -> (String, String) {
    let mut full = String::new();
    let mut initials = String::new();
    for ch in name.chars() {
        if let Some(py) = ch.to_pinyin() {
            let plain = py.plain();
            full.push_str(plain);
            if let Some(first) = plain.chars().next() {
                initials.push(first);
            }
        }
    }
    (full, initials)
}

fn score_text(text: &str, tokens: &[String], query: &str) -> i64 {
    if text == query {
        return 1000;
    }
    if text.starts_with(query) {
        return 800;
    }
    if tokens.iter().any(|token| token.starts_with(query)) {
        return 700;
    }
    if text.contains(query) {
        return 600;
    }

    let joined = tokens.join("");
    let initials: String = tokens.iter().filter_map(|token| token.chars().next()).collect();
    if joined.starts_with(query) || (!initials.is_empty() && initials.contains(query)) {
        return 500;
    }
    if is_subsequence(text, query) {
        return 300;
    }
    0
}

fn score_pinyin(name: &str, query: &str) -> i64 {
    let (full, initials) = pinyin_keys(name);
    if full.is_empty() {
        return 0;
    }
    if full == query || initials == query {
        return 900;
    }
    if full.starts_with(query) {
        return 760;
    }
    if initials.starts_with(query) {
        return 720;
    }
    if full.contains(query) || initials.contains(query) {
        return 520;
    }
    if is_subsequence(&full, query) || is_subsequence(&initials, query) {
        return 280;
    }
    0
}

pub fn score_application(app: &Application, keyword: &str) -> i64 {
    let query = keyword.trim().to_lowercase();
    if query.is_empty() {
        return 0;
    }

    let name = app.name.to_lowercase();
    let tokens = words(&app.name);
    score_text(&name, &tokens, &query).max(score_pinyin(&app.name, &query))
}

pub fn filter_applications(
    apps: &[Application],
    keyword: &str,
    limit: usize,
    enable_usage_ranking: bool,
) -> Vec<Application> {
    SearchIndex::build(apps.to_vec(), limit as i64, enable_usage_ranking).search(keyword)
}

#[derive(Debug, Clone)]
struct IndexedApplication {
    app: Application,
    name_lower: String,
    tokens: Vec<String>,
    token_joined: String,
    token_initials: String,
    pinyin_full: String,
    pinyin_initials: String,
}

impl IndexedApplication {
    fn from_app(app: Application) -> Self {
        let name_lower = app.name.to_lowercase();
        let tokens = words(&app.name);
        let token_joined = tokens.join("");
        let token_initials: String = tokens.iter().filter_map(|token| token.chars().next()).collect();
        let (pinyin_full, pinyin_initials) = pinyin_keys(&app.name);
        Self {
            app,
            name_lower,
            tokens,
            token_joined,
            token_initials,
            pinyin_full,
            pinyin_initials,
        }
    }

    fn score(&self, query: &str) -> i64 {
        score_text_indexed(self, query).max(score_pinyin_indexed(self, query))
    }
}

fn score_text_indexed(item: &IndexedApplication, query: &str) -> i64 {
    if item.name_lower == query {
        return 1000;
    }
    if item.name_lower.starts_with(query) {
        return 800;
    }
    if item.tokens.iter().any(|token| token.starts_with(query)) {
        return 700;
    }
    if item.name_lower.contains(query) {
        return 600;
    }
    if item.token_joined.starts_with(query)
        || (!item.token_initials.is_empty() && item.token_initials.contains(query))
    {
        return 500;
    }
    if is_subsequence(&item.name_lower, query) {
        return 300;
    }
    0
}

fn score_pinyin_indexed(item: &IndexedApplication, query: &str) -> i64 {
    if item.pinyin_full.is_empty() {
        return 0;
    }
    if item.pinyin_full == query || item.pinyin_initials == query {
        return 900;
    }
    if item.pinyin_full.starts_with(query) {
        return 760;
    }
    if item.pinyin_initials.starts_with(query) {
        return 720;
    }
    if item.pinyin_full.contains(query) || item.pinyin_initials.contains(query) {
        return 520;
    }
    if is_subsequence(&item.pinyin_full, query) || is_subsequence(&item.pinyin_initials, query) {
        return 280;
    }
    0
}

#[derive(Debug, Clone)]
pub struct SearchIndex {
    items: Vec<IndexedApplication>,
    result_limit: usize,
    enable_usage_ranking: bool,
}

impl SearchIndex {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            result_limit: 8,
            enable_usage_ranking: true,
        }
    }

    pub fn build(apps: Vec<Application>, result_limit: i64, enable_usage_ranking: bool) -> Self {
        let mut index = Self {
            items: apps.into_iter().map(IndexedApplication::from_app).collect(),
            result_limit: result_limit.max(1) as usize,
            enable_usage_ranking,
        };
        index.sort_recent();
        index
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn set_prefs(&mut self, result_limit: i64, enable_usage_ranking: bool) {
        self.result_limit = result_limit.max(1) as usize;
        self.enable_usage_ranking = enable_usage_ranking;
    }

    pub fn note_launch(&mut self, id: &str, time: i64) {
        if let Some(item) = self.items.iter_mut().find(|item| item.app.id == id) {
            item.app.launch_count += 1;
            item.app.last_launch_time = Some(time);
            self.sort_recent();
        }
    }

    pub fn search(&self, keyword: &str) -> Vec<Application> {
        let query = keyword.trim();
        if query.is_empty() {
            return self
                .items
                .iter()
                .take(self.result_limit)
                .map(|item| item.app.clone())
                .collect();
        }

        let query = query.to_lowercase();
        let mut scored: Vec<(i64, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let score = item.score(&query);
                (score > 0).then_some((score, index))
            })
            .collect();

        scored.sort_by(|left, right| {
            right.0.cmp(&left.0).then_with(|| {
                if self.enable_usage_ranking {
                    self.items[right.1]
                        .app
                        .launch_count
                        .cmp(&self.items[left.1].app.launch_count)
                } else {
                    std::cmp::Ordering::Equal
                }
            })
        });
        scored
            .into_iter()
            .take(self.result_limit)
            .map(|(_, index)| self.items[index].app.clone())
            .collect()
    }

    fn sort_recent(&mut self) {
        self.items.sort_by(|left, right| {
            right
                .app
                .last_launch_time
                .unwrap_or(0)
                .cmp(&left.app.last_launch_time.unwrap_or(0))
                .then(right.app.launch_count.cmp(&left.app.launch_count))
                .then(left.app.name.cmp(&right.app.name))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{filter_applications, score_application};
    use crate::database::models::Application;

    fn app(name: &str) -> Application {
        Application {
            id: name.into(),
            name: name.into(),
            path: name.into(),
            bundle_id: None,
            icon: None,
            source: "applications".into(),
            launch_count: 1,
            last_launch_time: Some(1),
        }
    }

    #[test]
    fn finds_chrome_by_chr() {
        let chrome = app("Google Chrome");
        assert!(score_application(&chrome, "chr") > 0);
    }

    #[test]
    fn finds_vscode_by_vsc_and_code() {
        let vscode = app("Visual Studio Code");
        assert!(score_application(&vscode, "vsc") > 0);
        assert!(score_application(&vscode, "code") > 0);
        assert!(score_application(&vscode, "vs") > 0);
    }

    #[test]
    fn empty_keyword_returns_recent() {
        let apps = vec![app("A"), app("B")];
        let result = filter_applications(&apps, "", 8, true);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn finds_wechat_by_pinyin() {
        let wechat = app("微信");
        assert!(score_application(&wechat, "微信") > 0);
        assert!(score_application(&wechat, "wx") > 0);
        assert!(score_application(&wechat, "weixin") > 0);
        assert!(score_application(&wechat, "wei") > 0);
        let work = app("企业微信");
        assert!(score_application(&work, "qywx") > 0);
    }

    #[test]
    fn search_1000_apps_under_20ms() {
        let apps: Vec<Application> = (0..1000)
            .map(|index| {
                let mut item = app(&format!("App {index:04} Visual Studio Code"));
                item.id = format!("app-{index}");
                item.launch_count = index % 17;
                item.last_launch_time = Some(index);
                item
            })
            .collect();
        let index = super::SearchIndex::build(apps, 8, true);
        let _ = index.search("vsc");
        let started = std::time::Instant::now();
        let result = index.search("vsc");
        let elapsed = started.elapsed();
        assert!(!result.is_empty());
        assert!(
            elapsed.as_millis() < 20,
            "1000 条搜索耗时 {}ms，目标 < 20ms",
            elapsed.as_millis()
        );
    }
}

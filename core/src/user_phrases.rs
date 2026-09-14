//! 使用者自訂詞庫：讓使用者自行定義「打一組注音 → 送出一段任意文字」的
//! 捷徑，例如把地址、姓名、電話設成自訂詞，方便在網頁／文件裡快速填寫，
//! 不必每次都重新完整輸入（見 `docs/PROJECT_PLAN.md` 之外新增的實務需求：
//! 常用資料快速填寫）。
//!
//! 跟 [`crate::dictionary::Dictionary`] 共用同一套 TSV 格式
//! （`注音<TAB>文字<TAB>詞頻`），但額外支援在執行期新增／刪除詞條，並在
//! 有記錄來源檔案路徑時，每次異動都立即持久化寫回，重開輸入法後仍在。
//!
//! 自訂詞固定給很高的詞頻（[`USER_PHRASE_FREQUENCY`]），確保排序在候選
//! 字清單最前面（見 [`crate::Engine::refresh_candidates`]）——使用者特地
//! 自己設定的捷徑，理應比詞庫自動猜測的結果更優先出現。
//!
//! 目前新增／刪除詞條只有 [`UserPhrases::add`]／[`UserPhrases::remove`]
//! 這組程式介面；`backend/` 尚未提供透過 PIME 介面新增自訂詞的功能
//! （PIME 的線路協定沒有通用文字輸入框，見 `docs/PIME_PROTOCOL.md`），
//! 目前的使用方式是直接編輯自訂詞檔案（見 `README.md`）。

use crate::dictionary::Entry;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 自訂詞固定詞頻：刻意設得比一般詞庫詞頻都高（詞庫詞頻是真實語料統計，
/// 通常不會超過幾萬），確保自訂詞永遠排在候選字清單最前面。
pub const USER_PHRASE_FREQUENCY: u32 = 90_000;

/// 使用者自訂詞庫：注音字串 -> 自訂文字清單。
#[derive(Debug, Default, Clone)]
pub struct UserPhrases {
    entries: HashMap<String, Vec<Entry>>,
    /// 載入來源檔案路徑；`None` 代表純記憶體、不會持久化（例如測試，或
    /// 尚未指定檔案位置時的預設狀態）。
    path: Option<PathBuf>,
}

impl UserPhrases {
    pub fn new() -> Self {
        Self::default()
    }

    /// 從檔案載入。檔案不存在時視為「尚無任何自訂詞」而不是錯誤，讓
    /// 使用者第一次使用時不必手動建立空檔案；之後呼叫 [`UserPhrases::add`]
    /// ／[`UserPhrases::remove`] 會寫回這個路徑。
    pub fn load_file(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err),
        };
        let mut phrases = Self::parse(&content);
        phrases.path = Some(path);
        Ok(phrases)
    }

    /// 從字串解析，格式同 [`UserPhrases::load_file`]；純記憶體、不記錄
    /// 來源路徑，之後的 `add`／`remove` 不會嘗試寫檔。
    pub fn parse(content: &str) -> Self {
        let mut entries: HashMap<String, Vec<Entry>> = HashMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t');
            let (Some(code), Some(text)) = (fields.next(), fields.next()) else {
                continue;
            };
            entries.entry(code.to_string()).or_default().push(Entry {
                word: text.to_string(),
                frequency: USER_PHRASE_FREQUENCY,
            });
        }
        Self {
            entries,
            path: None,
        }
    }

    /// 依注音字串查詢自訂詞，找不到時回傳空陣列。
    pub fn lookup(&self, code: &str) -> &[Entry] {
        self.entries.get(code).map(Vec::as_slice).unwrap_or(&[])
    }

    /// 新增一筆自訂詞（同一個注音字串可以對應多筆文字；同一個注音＋
    /// 文字的組合重複新增不會產生重複項目）。若載入時記錄了來源路徑，
    /// 這裡新增後會立即寫回檔案。
    pub fn add(&mut self, code: &str, text: &str) -> io::Result<()> {
        let list = self.entries.entry(code.to_string()).or_default();
        let already_present = list.iter().any(|entry| entry.word == text);
        if already_present {
            return Ok(());
        }
        list.push(Entry {
            word: text.to_string(),
            frequency: USER_PHRASE_FREQUENCY,
        });
        self.save()
    }

    /// 移除一筆自訂詞，回傳是否真的有東西被移除。若載入時記錄了來源
    /// 路徑，這裡移除後會立即寫回檔案。
    pub fn remove(&mut self, code: &str, text: &str) -> io::Result<bool> {
        let mut removed = false;
        if let Some(list) = self.entries.get_mut(code) {
            let before = list.len();
            list.retain(|entry| entry.word != text);
            removed = list.len() != before;
            if list.is_empty() {
                self.entries.remove(code);
            }
        }
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// 目前所有自訂詞（注音字串, 候選字），依注音字串排序，供列出／管理
    /// 用（例如未來的設定介面）。
    pub fn list(&self) -> Vec<(&str, &Entry)> {
        let mut codes: Vec<&String> = self.entries.keys().collect();
        codes.sort();
        codes
            .into_iter()
            .flat_map(|code| {
                self.entries[code]
                    .iter()
                    .map(move |entry| (code.as_str(), entry))
            })
            .collect()
    }

    /// 自訂詞條總數。
    pub fn len(&self) -> usize {
        self.entries.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 把目前所有自訂詞寫回載入時記錄的來源路徑；若是純記憶體
    /// （[`UserPhrases::parse`] 建立、或 [`UserPhrases::new`]）則是no-op。
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        self.save_to(path)
    }

    fn save_to(&self, path: &Path) -> io::Result<()> {
        let mut codes: Vec<&String> = self.entries.keys().collect();
        codes.sort();
        let mut content = String::new();
        for code in codes {
            for entry in &self.entries[code] {
                content.push_str(code);
                content.push('\t');
                content.push_str(&entry.word);
                content.push('\t');
                content.push_str(&entry.frequency.to_string());
                content.push('\n');
            }
        }
        fs::write(path, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "zuyin-user-phrases-test-{name}-{}.txt",
            std::process::id()
        ))
    }

    #[test]
    fn parses_valid_lines_and_skips_comments_and_blanks() {
        let phrases = UserPhrases::parse("# 自訂詞\n\nㄉㄓ\t台北市大安區羅斯福路四段1號\t90000\n");
        assert_eq!(phrases.len(), 1);
        assert_eq!(
            phrases.lookup("ㄉㄓ"),
            &[Entry {
                word: "台北市大安區羅斯福路四段1號".into(),
                frequency: USER_PHRASE_FREQUENCY
            }]
        );
    }

    #[test]
    fn missing_key_returns_empty_slice() {
        let phrases = UserPhrases::new();
        assert!(phrases.lookup("ㄉㄓ").is_empty());
    }

    #[test]
    fn add_makes_the_phrase_immediately_queryable() {
        let mut phrases = UserPhrases::new();
        phrases.add("ㄉㄓ", "台北市大安區").unwrap();
        assert_eq!(
            phrases.lookup("ㄉㄓ"),
            &[Entry {
                word: "台北市大安區".into(),
                frequency: USER_PHRASE_FREQUENCY
            }]
        );
    }

    #[test]
    fn adding_the_same_code_and_text_twice_does_not_duplicate() {
        let mut phrases = UserPhrases::new();
        phrases.add("ㄉㄓ", "台北市大安區").unwrap();
        phrases.add("ㄉㄓ", "台北市大安區").unwrap();
        assert_eq!(phrases.lookup("ㄉㄓ").len(), 1);
    }

    #[test]
    fn same_code_can_have_multiple_texts() {
        let mut phrases = UserPhrases::new();
        phrases.add("ㄉㄓ", "台北市大安區").unwrap();
        phrases.add("ㄉㄓ", "台北市信義區").unwrap();
        assert_eq!(phrases.lookup("ㄉㄓ").len(), 2);
    }

    #[test]
    fn remove_deletes_the_phrase_and_reports_success() {
        let mut phrases = UserPhrases::new();
        phrases.add("ㄉㄓ", "台北市大安區").unwrap();
        assert!(phrases.remove("ㄉㄓ", "台北市大安區").unwrap());
        assert!(phrases.lookup("ㄉㄓ").is_empty());
    }

    #[test]
    fn removing_something_that_is_not_there_reports_false() {
        let mut phrases = UserPhrases::new();
        assert!(!phrases.remove("ㄉㄓ", "台北市大安區").unwrap());
    }

    #[test]
    fn saving_without_a_source_path_is_a_no_op() {
        let phrases = UserPhrases::parse("ㄉㄓ\t台北市大安區\t90000\n");
        assert!(phrases.save().is_ok());
    }

    #[test]
    fn load_file_on_a_missing_file_starts_empty_but_remembers_the_path() {
        let path = temp_path("load-missing");
        let _ = fs::remove_file(&path);
        let mut phrases = UserPhrases::load_file(&path).unwrap();
        assert!(phrases.is_empty());

        // 之後新增應該會建立這個檔案。
        phrases.add("ㄉㄓ", "台北市大安區").unwrap();
        assert!(path.exists());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn add_and_remove_persist_to_the_loaded_file() {
        let path = temp_path("persist-roundtrip");
        let _ = fs::remove_file(&path);

        let mut phrases = UserPhrases::load_file(&path).unwrap();
        phrases.add("ㄒㄇ", "王小明").unwrap();
        phrases.add("ㄉㄏ", "0912-345-678").unwrap();

        // 重新從檔案載入，確認真的寫回去了。
        let reloaded = UserPhrases::load_file(&path).unwrap();
        assert_eq!(
            reloaded.lookup("ㄒㄇ"),
            &[Entry {
                word: "王小明".into(),
                frequency: USER_PHRASE_FREQUENCY
            }]
        );
        assert_eq!(reloaded.lookup("ㄉㄏ").len(), 1);

        let mut phrases = reloaded;
        phrases.remove("ㄒㄇ", "王小明").unwrap();
        let reloaded_again = UserPhrases::load_file(&path).unwrap();
        assert!(reloaded_again.lookup("ㄒㄇ").is_empty());
        assert_eq!(reloaded_again.lookup("ㄉㄏ").len(), 1);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn list_is_sorted_by_code() {
        let mut phrases = UserPhrases::new();
        phrases.add("ㄒㄇ", "王小明").unwrap();
        phrases.add("ㄉㄓ", "台北市大安區").unwrap();
        let codes: Vec<&str> = phrases.list().into_iter().map(|(code, _)| code).collect();
        assert_eq!(codes, vec!["ㄉㄓ", "ㄒㄇ"]);
    }
}

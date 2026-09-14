//! 詞庫查詢：讀取「注音 → 候選字」對照表，並依注音字串查詢候選字。
//!
//! 詞庫檔案格式：每行 `注音<TAB>詞<TAB>詞頻`，`#` 開頭或空白行會被忽略。
//! Phase 1 建議直接採用新酷音或 RIME 現成公開詞庫轉換而來（見
//! `docs/PROJECT_PLAN.md` 五、風險與備註）。

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

/// 詞庫中的一筆候選字（詞）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub word: String,
    pub frequency: u32,
}

/// 以注音字串為鍵的詞庫。
#[derive(Debug, Default, Clone)]
pub struct Dictionary {
    entries: HashMap<String, Vec<Entry>>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    /// 從檔案載入詞庫。
    pub fn load_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(Self::parse(&content))
    }

    /// 從字串解析詞庫，格式同 [`Dictionary::load_file`]。
    pub fn parse(content: &str) -> Self {
        let mut entries: HashMap<String, Vec<Entry>> = HashMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t');
            let (Some(zhuyin), Some(word), Some(freq)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let Ok(frequency) = freq.trim().parse::<u32>() else {
                continue;
            };
            entries.entry(zhuyin.to_string()).or_default().push(Entry {
                word: word.to_string(),
                frequency,
            });
        }
        Self { entries }
    }

    /// 依注音字串查詢候選字，找不到時回傳空陣列。
    pub fn lookup(&self, zhuyin: &str) -> &[Entry] {
        self.entries.get(zhuyin).map(Vec::as_slice).unwrap_or(&[])
    }

    /// 詞庫中的候選字（詞）總數。
    pub fn len(&self) -> usize {
        self.entries.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_lines_and_skips_comments_and_blanks() {
        let dict =
            Dictionary::parse("# 範例詞庫\n\nㄋㄧˇ\t你\t5000\nㄏㄠˇ\t好\t5000\nㄏㄠˇ\t號\t100\n");
        assert_eq!(dict.len(), 3);
        assert_eq!(
            dict.lookup("ㄋㄧˇ"),
            &[Entry {
                word: "你".into(),
                frequency: 5000
            }]
        );
        assert_eq!(
            dict.lookup("ㄏㄠˇ"),
            &[
                Entry {
                    word: "好".into(),
                    frequency: 5000
                },
                Entry {
                    word: "號".into(),
                    frequency: 100
                },
            ]
        );
    }

    #[test]
    fn missing_key_returns_empty_slice() {
        let dict = Dictionary::parse("ㄋㄧˇ\t你\t5000\n");
        assert!(dict.lookup("ㄕˋ").is_empty());
    }

    #[test]
    fn malformed_lines_are_ignored() {
        let dict = Dictionary::parse("ㄋㄧˇ\t你\tnot-a-number\nㄏㄠˇ\t好\n");
        assert!(dict.is_empty());
    }
}

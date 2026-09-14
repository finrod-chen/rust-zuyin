//! 候選字排序：依詞庫詞頻與使用者個人選字記憶排序候選字。

use crate::dictionary::Entry;
use std::collections::HashMap;

/// 使用者每次選字後，記憶分數的增量。
const SELECTION_BOOST: u32 = 100;

/// 追蹤使用者選字記憶，並依此調整候選字排序。
#[derive(Debug, Default, Clone)]
pub struct Ranker {
    /// key: (注音字串, 詞) → 使用者記憶加權分數
    memory: HashMap<(String, String), u32>,
}

impl Ranker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用者選定某候選字後呼叫，之後同音節下該詞的排序會提高。
    pub fn record_selection(&mut self, zhuyin: &str, word: &str) {
        let key = (zhuyin.to_string(), word.to_string());
        *self.memory.entry(key).or_insert(0) += SELECTION_BOOST;
    }

    /// 清除所有已累積的使用者選字記憶，排序退回純依詞庫詞頻。
    pub fn clear(&mut self) {
        self.memory.clear();
    }

    /// 依「詞庫詞頻 + 使用者記憶分數」由高到低排序候選字；
    /// 分數相同時保留詞庫原本的相對順序（stable sort）。
    pub fn rank<'a>(&self, zhuyin: &str, entries: &'a [Entry]) -> Vec<&'a Entry> {
        let mut ranked: Vec<&Entry> = entries.iter().collect();
        ranked.sort_by_key(|entry| {
            let boost = self
                .memory
                .get(&(zhuyin.to_string(), entry.word.clone()))
                .copied()
                .unwrap_or(0);
            std::cmp::Reverse(entry.frequency.saturating_add(boost))
        });
        ranked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        vec![
            Entry {
                word: "好".into(),
                frequency: 250,
            },
            Entry {
                word: "號".into(),
                frequency: 100,
            },
        ]
    }

    #[test]
    fn ranks_by_base_frequency_without_history() {
        let ranker = Ranker::new();
        let entries = entries();
        let ranked = ranker.rank("ㄏㄠˇ", &entries);
        assert_eq!(
            ranked.iter().map(|e| e.word.as_str()).collect::<Vec<_>>(),
            vec!["好", "號"]
        );
    }

    #[test]
    fn selection_history_can_promote_lower_frequency_word() {
        let mut ranker = Ranker::new();
        // 使用者連續兩次選「號」，累積加權超過「好」的基礎詞頻差距
        ranker.record_selection("ㄏㄠˇ", "號");
        ranker.record_selection("ㄏㄠˇ", "號");
        let entries = entries();
        let ranked = ranker.rank("ㄏㄠˇ", &entries);
        assert_eq!(
            ranked.iter().map(|e| e.word.as_str()).collect::<Vec<_>>(),
            vec!["號", "好"]
        );
    }

    #[test]
    fn history_is_scoped_per_syllable() {
        let mut ranker = Ranker::new();
        ranker.record_selection("ㄏㄠˋ", "號"); // 不同音節（第四聲）的記憶不應影響此查詢
        let entries = entries();
        let ranked = ranker.rank("ㄏㄠˇ", &entries);
        assert_eq!(
            ranked.iter().map(|e| e.word.as_str()).collect::<Vec<_>>(),
            vec!["好", "號"]
        );
    }

    #[test]
    fn clear_resets_ranking_to_base_frequency() {
        let mut ranker = Ranker::new();
        ranker.record_selection("ㄏㄠˇ", "號");
        ranker.record_selection("ㄏㄠˇ", "號");
        ranker.clear();
        let entries = entries();
        let ranked = ranker.rank("ㄏㄠˇ", &entries);
        assert_eq!(
            ranked.iter().map(|e| e.word.as_str()).collect::<Vec<_>>(),
            vec!["好", "號"]
        );
    }
}

//! 詞庫查詢：讀取「注音 → 候選字」對照表，並依注音字串查詢候選字。
//!
//! 詞庫檔案格式：每行 `注音<TAB>詞<TAB>詞頻`，`#` 開頭或空白行會被忽略。
//! Phase 1 建議直接採用新酷音或 RIME 現成公開詞庫轉換而來（見
//! `docs/PROJECT_PLAN.md` 五、風險與備註；實際轉換見
//! `scripts/convert_chewing_dictionary.py`）。
//!
//! 多字詞（片語）的注音欄位以空白分隔每個音節，例如「你好」是
//! `"ㄋㄧˇ ㄏㄠˇ"`；單一音節的詞條沒有空白，天然相容同一套格式。這個
//! 空白同時是 [`Dictionary::is_valid_prefix`] 判斷音節邊界的依據——
//! `core::Engine` 用它來決定「使用者連續打的這幾個音節，是否還有機會
//! 湊成詞庫裡的某個詞」，藉此在最長匹配失敗時知道該在哪裡收手。
//!
//! ## 注音縮寫輸入（仿手機輸入法）
//!
//! 每個音節字串本身就依「聲母 → 介母 → 韻母 → 聲調」排序（見
//! [`crate::syllable::Syllable::as_zhuyin_string`]），所以音節字串的第一
//! 個字元，天然就是這個音節「第一個打的符號」（聲母；沒聲母則是介母；
//! 都沒有才是韻母——聲調恆在最後，不會是第一個字元）。把一個詞每個音節
//! 的第一個字元依序串起來，就是這個詞的「縮寫碼」，例如「謝謝」
//! （ㄒㄧㄝˋ ㄒㄧㄝˋ）的縮寫碼是「ㄒㄒ」。[`Dictionary::lookup_abbreviation`]
//! 用這個縮寫碼查詞，讓使用者只打每個字的第一個符號就能叫出候選字，
//! 只建立在至少兩個音節的詞條上（見 [`Dictionary::parse`]），單音節詞
//! 不會被收進這個索引，避免縮寫查詢被大量單字候選字淹沒。
//!
//! ## 不分聲調選字
//!
//! 使用者常常只想打完聲母／介母／韻母、不特別指定聲調就選字（尤其是
//! 記不清或懶得打聲調的時候）。[`Dictionary::lookup_toneless`] 用「拿掉
//! 每個音節聲調後的字串」（`toneless_index`，見
//! [`crate::keyboard::TONE_MARKS`]）當鍵，把同一個基底讀音、不同聲調的
//! 候選字全部找出來，讓使用者不必先打聲調才能選字。

use crate::keyboard::TONE_MARKS;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;

/// 詞庫中的一筆候選字（詞）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub word: String,
    pub frequency: u32,
}

/// 以注音字串（單音節或以空白分隔的多音節）為鍵的詞庫。
#[derive(Debug, Default, Clone)]
pub struct Dictionary {
    entries: HashMap<String, Vec<Entry>>,
    /// 詞庫中每個詞條的音節前綴集合（見模組說明）。
    valid_prefixes: HashSet<String>,
    /// 縮寫碼（每個音節的第一個符號串起來） -> 候選字，只收多音節詞條
    /// （見模組說明「注音縮寫輸入」）。
    abbreviation_index: HashMap<String, Vec<Entry>>,
    /// 拿掉每個音節聲調後的字串 -> 候選字（見模組說明「不分聲調選字」）。
    toneless_index: HashMap<String, Vec<Entry>>,
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
        let mut valid_prefixes: HashSet<String> = HashSet::new();
        let mut abbreviation_index: HashMap<String, Vec<Entry>> = HashMap::new();
        let mut toneless_index: HashMap<String, Vec<Entry>> = HashMap::new();
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

            let mut prefix = String::new();
            let mut syllable_count = 0;
            let mut abbreviation_code = String::new();
            let mut toneless_parts: Vec<&str> = Vec::new();
            for syllable in zhuyin.split(' ') {
                if !prefix.is_empty() {
                    prefix.push(' ');
                }
                prefix.push_str(syllable);
                valid_prefixes.insert(prefix.clone());
                syllable_count += 1;
                if let Some(leading) = syllable.chars().next() {
                    abbreviation_code.push(leading);
                }
                toneless_parts.push(strip_tone(syllable));
            }

            let entry = Entry {
                word: word.to_string(),
                frequency,
            };
            if syllable_count >= 2 {
                abbreviation_index
                    .entry(abbreviation_code)
                    .or_default()
                    .push(entry.clone());
            }
            toneless_index
                .entry(toneless_parts.join(" "))
                .or_default()
                .push(entry.clone());
            entries.entry(zhuyin.to_string()).or_default().push(entry);
        }
        Self {
            entries,
            valid_prefixes,
            abbreviation_index,
            toneless_index,
        }
    }

    /// 依注音字串查詢候選字，找不到時回傳空陣列。
    pub fn lookup(&self, zhuyin: &str) -> &[Entry] {
        self.entries.get(zhuyin).map(Vec::as_slice).unwrap_or(&[])
    }

    /// 這串以空白分隔的音節序列，是否仍是詞庫裡某個詞條的合法前綴
    /// （詞條本身也算自己的前綴，所以完整詞條在這裡也會回傳 `true`）。
    ///
    /// 用來讓 [`crate::Engine`] 判斷：使用者打完目前這個音節後，是要
    /// 「繼續累積、嘗試組出更長的詞」，還是「這個音節已經沒辦法接在
    /// 前面湊成任何詞了，該收手」。
    pub fn is_valid_prefix(&self, syllables: &str) -> bool {
        self.valid_prefixes.contains(syllables)
    }

    /// 依縮寫碼（每個音節的第一個符號串起來，見模組說明「注音縮寫
    /// 輸入」）查詢候選字，找不到時回傳空陣列。只有多音節詞條才會被
    /// 縮寫碼收錄，所以單一符號的縮寫碼必定查不到任何結果。
    pub fn lookup_abbreviation(&self, code: &str) -> &[Entry] {
        self.abbreviation_index
            .get(code)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// 依「拿掉聲調的讀音」（見模組說明「不分聲調選字」）查詢候選字，
    /// 找不到時回傳空陣列。`base` 應該是每個音節只有聲母／介母／韻母、
    /// 以空白分隔的字串（見
    /// [`crate::syllable::Syllable::base_zhuyin_string`]）。
    pub fn lookup_toneless(&self, base: &str) -> &[Entry] {
        self.toneless_index
            .get(base)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// 詞庫中的候選字（詞）總數。
    pub fn len(&self) -> usize {
        self.entries.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// 拿掉一個音節字串結尾的聲調符號（若有）。聲調恆是
/// [`crate::syllable::Syllable::as_zhuyin_string`] 的最後一個字元，所以
/// 只需要檢查最後一個字元是否屬於 [`TONE_MARKS`]。
fn strip_tone(syllable: &str) -> &str {
    match syllable.chars().next_back() {
        Some(last) if TONE_MARKS.contains(&last) => &syllable[..syllable.len() - last.len_utf8()],
        _ => syllable,
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

    #[test]
    fn multi_syllable_entry_is_looked_up_by_space_joined_key() {
        let dict = Dictionary::parse("ㄋㄧˇ ㄏㄠˇ\t你好\t1227\n");
        assert_eq!(
            dict.lookup("ㄋㄧˇ ㄏㄠˇ"),
            &[Entry {
                word: "你好".into(),
                frequency: 1227
            }]
        );
    }

    #[test]
    fn single_syllable_prefix_of_a_phrase_is_valid() {
        let dict = Dictionary::parse("ㄋㄧˇ ㄏㄠˇ\t你好\t1227\n");
        assert!(
            dict.is_valid_prefix("ㄋㄧˇ"),
            "第一個音節本身就是「你好」的合法前綴"
        );
        assert!(
            dict.is_valid_prefix("ㄋㄧˇ ㄏㄠˇ"),
            "完整詞條本身也算自己的前綴"
        );
    }

    #[test]
    fn unrelated_syllable_is_not_a_valid_prefix() {
        let dict = Dictionary::parse("ㄋㄧˇ ㄏㄠˇ\t你好\t1227\n");
        assert!(
            !dict.is_valid_prefix("ㄕˋ"),
            "「是」跟「你好」無關，不該是合法前綴"
        );
        assert!(
            !dict.is_valid_prefix("ㄋㄧˇ ㄕˋ"),
            "「你」後面接「是」湊不出詞庫裡的任何詞"
        );
    }

    #[test]
    fn a_syllable_that_is_only_a_standalone_character_is_not_a_prefix_of_anything_longer() {
        // 「是」只有單字詞條，沒有以它開頭的更長詞，所以它是自己的前綴，
        // 但不該讓後面接任何音節都被誤判成「還有機會湊成詞」。
        let dict = Dictionary::parse("ㄕˋ\t是\t9000\nㄋㄧˇ ㄏㄠˇ\t你好\t1227\n");
        assert!(dict.is_valid_prefix("ㄕˋ"));
        assert!(!dict.is_valid_prefix("ㄕˋ ㄋㄧˇ"));
    }

    #[test]
    fn abbreviation_lookup_finds_all_words_sharing_the_same_leading_glyphs() {
        // 謝謝／熊熊／行銷 三個詞的兩個音節開頭都是 ㄒ，縮寫碼都是「ㄒㄒ」。
        let dict = Dictionary::parse(
            "ㄒㄧㄝˋ ㄒㄧㄝˋ\t謝謝\t500\n\
             ㄒㄩㄥˊ ㄒㄩㄥˊ\t熊熊\t100\n\
             ㄒㄧㄥˊ ㄒㄧㄠ\t行銷\t800\n\
             ㄋㄧˇ ㄏㄠˇ\t你好\t1227\n",
        );
        let mut words: Vec<&str> = dict
            .lookup_abbreviation("ㄒㄒ")
            .iter()
            .map(|e| e.word.as_str())
            .collect();
        words.sort();
        assert_eq!(words, vec!["熊熊", "行銷", "謝謝"]);
    }

    #[test]
    fn abbreviation_index_ignores_single_syllable_entries() {
        // 單音節詞條不該被收進縮寫索引，否則隨便打一個聲母就會被灌爆。
        let dict = Dictionary::parse("ㄒㄧˋ\t係\t100\n");
        assert!(dict.lookup_abbreviation("ㄒ").is_empty());
    }

    #[test]
    fn abbreviation_lookup_with_no_match_returns_empty_slice() {
        let dict = Dictionary::parse("ㄋㄧˇ ㄏㄠˇ\t你好\t1227\n");
        assert!(dict.lookup_abbreviation("ㄒㄒ").is_empty());
    }

    #[test]
    fn toneless_lookup_finds_words_across_every_tone_of_the_same_base_reading() {
        let dict = Dictionary::parse(
            "ㄊㄞˊ\t台\t3000\n\
             ㄊㄞˋ\t太\t5000\n\
             ㄊㄞ\t胎\t500\n\
             ㄏㄠˇ\t好\t9000\n",
        );
        let mut words: Vec<&str> = dict
            .lookup_toneless("ㄊㄞ")
            .iter()
            .map(|e| e.word.as_str())
            .collect();
        words.sort();
        assert_eq!(words, vec!["台", "太", "胎"]);
    }

    #[test]
    fn toneless_lookup_also_works_across_multi_syllable_phrases() {
        let dict = Dictionary::parse("ㄋㄧˇ ㄏㄠˇ\t你好\t1227\n");
        assert_eq!(
            dict.lookup_toneless("ㄋㄧ ㄏㄠ"),
            &[Entry {
                word: "你好".into(),
                frequency: 1227
            }]
        );
    }

    #[test]
    fn toneless_lookup_with_no_match_returns_empty_slice() {
        let dict = Dictionary::parse("ㄊㄞˊ\t台\t3000\n");
        assert!(dict.lookup_toneless("ㄏㄠ").is_empty());
    }
}

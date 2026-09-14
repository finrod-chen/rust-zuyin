//! Zhuyin（注音）輸入法核心引擎。
//!
//! 此 crate 為平台無關的純邏輯 library，不依賴任何特定作業系統 GUI 或
//! 輸入法框架（如 PIME／TSF），方便獨立測試與優化，也方便未來擴充到其他
//! 平台。詳見 `docs/PROJECT_PLAN.md`。

pub mod dictionary;
pub mod keyboard;
pub mod ranking;
pub mod syllable;

pub use dictionary::{Dictionary, Entry};

use keyboard::StandardLayout;
use ranking::Ranker;
use syllable::{PushResult, Syllable};

/// 呼叫端送入一個按鍵後，引擎的回應。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    /// 按鍵不屬於注音鍵盤，呼叫端應自行處理（例如直接輸入該字元、或視為
    /// 一般英數字元送出）。
    NotHandled,
    /// 按鍵已接受，組字區內容如下；候選字清單依目前音節查詢並排序。
    Composing {
        buffer: String,
        candidates: Vec<Entry>,
    },
}

/// 注音輸入法核心引擎：組合鍵盤佈局、音節狀態機、詞庫與排序模型。
pub struct Engine {
    layout: StandardLayout,
    syllable: Syllable,
    dictionary: Dictionary,
    ranker: Ranker,
}

impl Engine {
    pub fn new(dictionary: Dictionary) -> Self {
        Self {
            layout: StandardLayout,
            syllable: Syllable::new(),
            dictionary,
            ranker: Ranker::new(),
        }
    }

    /// 目前組字區的注音字串。
    pub fn buffer(&self) -> String {
        self.syllable.as_zhuyin_string()
    }

    /// 處理一個按鍵事件。
    pub fn key_press(&mut self, key: char) -> KeyOutcome {
        let Some(symbol) = self.layout.lookup(key) else {
            return KeyOutcome::NotHandled;
        };

        if self.syllable.push(symbol) == PushResult::Rejected {
            // 同類別符號已填過：視為使用者要開始下一個音節，重打這一鍵。
            self.syllable.clear();
            self.syllable.push(symbol);
        }

        self.refresh_candidates()
    }

    /// 刪除最後輸入的符號。
    pub fn backspace(&mut self) -> KeyOutcome {
        self.syllable.backspace();
        self.refresh_candidates()
    }

    /// 清空目前組字狀態（例如使用者按 Esc）。
    pub fn clear(&mut self) {
        self.syllable.clear();
    }

    /// 使用者確認選字：記錄使用者記憶並清空組字狀態，回傳應送入應用程式的文字。
    pub fn select_candidate(&mut self, word: &str) -> String {
        let zhuyin = self.syllable.as_zhuyin_string();
        self.ranker.record_selection(&zhuyin, word);
        self.syllable.clear();
        word.to_string()
    }

    fn refresh_candidates(&self) -> KeyOutcome {
        let buffer = self.syllable.as_zhuyin_string();
        let candidates = if self.syllable.is_ready() {
            let entries = self.dictionary.lookup(&buffer);
            self.ranker
                .rank(&buffer, entries)
                .into_iter()
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        KeyOutcome::Composing { buffer, candidates }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dictionary() -> Dictionary {
        Dictionary::parse(
            "ㄋㄧˇ\t你\t5000\n\
             ㄏㄠˇ\t好\t5000\n\
             ㄕˋ\t是\t9000\n\
             ㄨㄛˇ\t我\t9000\n\
             ㄊㄞˊ\t台\t3000\n\
             ㄨㄢ\t灣\t2000\n",
        )
    }

    #[test]
    fn typing_ni_then_hao_yields_expected_candidates() {
        let mut engine = Engine::new(sample_dictionary());

        // 你 = ㄋㄧˇ : s(ㄋ) u(ㄧ) 3(ˇ)
        engine.key_press('s');
        engine.key_press('u');
        let outcome = engine.key_press('3');
        match outcome {
            KeyOutcome::Composing { buffer, candidates } => {
                assert_eq!(buffer, "ㄋㄧˇ");
                assert_eq!(
                    candidates,
                    vec![Entry {
                        word: "你".into(),
                        frequency: 5000
                    }]
                );
            }
            other => panic!("unexpected outcome: {other:?}"),
        }

        let committed = engine.select_candidate("你");
        assert_eq!(committed, "你");
        assert_eq!(engine.buffer(), "");

        // 好 = ㄏㄠˇ : c(ㄏ) l(ㄠ) 3(ˇ)
        engine.key_press('c');
        engine.key_press('l');
        let outcome = engine.key_press('3');
        match outcome {
            KeyOutcome::Composing { buffer, candidates } => {
                assert_eq!(buffer, "ㄏㄠˇ");
                assert_eq!(
                    candidates,
                    vec![Entry {
                        word: "好".into(),
                        frequency: 5000
                    }]
                );
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn tai_wan_round_trips_through_full_pipeline() {
        let mut engine = Engine::new(sample_dictionary());

        // 台 = ㄊㄞˊ : w(ㄊ) 9(ㄞ) 6(ˊ)
        engine.key_press('w');
        engine.key_press('9');
        let outcome = engine.key_press('6');
        assert_eq!(
            outcome,
            KeyOutcome::Composing {
                buffer: "ㄊㄞˊ".into(),
                candidates: vec![Entry {
                    word: "台".into(),
                    frequency: 3000
                }],
            }
        );
        engine.select_candidate("台");

        // 灣 = ㄨㄢ (第一聲無聲調符號) : j(ㄨ) 0(ㄢ)
        engine.key_press('j');
        let outcome = engine.key_press('0');
        assert_eq!(
            outcome,
            KeyOutcome::Composing {
                buffer: "ㄨㄢ".into(),
                candidates: vec![Entry {
                    word: "灣".into(),
                    frequency: 2000
                }],
            }
        );
    }

    #[test]
    fn non_zhuyin_key_is_not_handled() {
        let mut engine = Engine::new(Dictionary::new());
        assert_eq!(engine.key_press('!'), KeyOutcome::NotHandled);
    }

    #[test]
    fn selecting_a_candidate_boosts_it_above_base_frequency_ranking_next_time() {
        let dict = Dictionary::parse("ㄏㄠˇ\t好\t250\nㄏㄠˇ\t號\t100\n");
        let mut engine = Engine::new(dict);

        engine.key_press('c');
        engine.key_press('l');
        engine.key_press('3');
        engine.select_candidate("號"); // 使用者選了詞頻較低的「號」

        // 每次確認選字都會清空組字區，所以要重打一次音節才能再記錄一次選字。
        engine.key_press('c');
        engine.key_press('l');
        engine.key_press('3');
        engine.select_candidate("號"); // 再選一次以確保加權足以超越基礎詞頻差距

        engine.key_press('c');
        engine.key_press('l');
        let outcome = engine.key_press('3');
        match outcome {
            KeyOutcome::Composing { candidates, .. } => {
                assert_eq!(candidates[0].word, "號", "使用者記憶應提升「號」的排序");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
}

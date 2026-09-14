//! 每個 TSF client session 對應一個 [`Session`]，包裝 zuyin-core 的
//! `Engine`，並實作 PIME `TextService` 的按鍵處理語意：
//!
//! - `filterKeyDown` 只能「預測」這個鍵是否要吃下，**不可**改變組字狀態
//!   （TSF 會先呼叫 filter 詢問，使用者同時可能有其他 client 在監聽同一
//!   個按鍵事件）
//! - `onKeyDown` 才實際處理按鍵、改變組字狀態
//!
//! 兩者對同一個按鍵必須給出一致的「吃／不吃」判斷，因此都透過同一個純
//! 函式 [`Session::classify`] 決策，只有 `on_key_down` 才會真的呼叫
//! engine 的變動方法。

use zuyin_core::{Dictionary, Engine, KeyOutcome};

use crate::protocol::KeyEventData;

const VK_BACK: u32 = 0x08;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_RETURN: u32 = 0x0D;
const VK_KEY_1: u32 = 0x31;
const VK_KEY_9: u32 = 0x39;

/// 按鍵事件依目前組字狀態應被歸類的動作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    /// 送入音節狀態機的注音符號按鍵。
    Symbol(char),
    /// 從候選字清單選字（0-based index），僅在候選字視窗開啟時才會出現。
    SelectCandidate(usize),
    /// 確認目前排序第一的候選字（空白／Enter，且候選字視窗開啟時）。
    CommitTop,
    Backspace,
    /// Esc：清空目前組字狀態。
    Clear,
    /// 這個 IME 不處理此按鍵，應直接交還給應用程式。
    PassThrough,
}

/// `on_key_down` 處理完一個按鍵後，呼叫端（`main.rs`）要據此組出 PIME 回應。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyDownOutcome {
    /// 音節組字狀態有變化（含候選字清單更新）。
    Composing {
        buffer: String,
        candidates: Vec<String>,
        show_candidates: bool,
    },
    /// 使用者確認選字，`commit_string` 應送入應用程式。
    Committed(String),
    /// 使用者按 Esc 清空組字區；組字區與候選字視窗皆應清空。
    Cleared,
    /// 未被此輸入法處理，應交還應用程式（不應設定任何組字／候選字欄位）。
    PassThrough,
}

pub struct Session {
    engine: Engine,
    is_activated: bool,
    /// 候選字視窗是否開啟；只有開啟時數字鍵才代表選字，而非注音符號。
    show_candidates: bool,
    /// 目前顯示的候選字，索引對應數字鍵 1-9。
    last_candidates: Vec<String>,
}

impl Session {
    pub fn new(dictionary: Dictionary) -> Self {
        Self {
            engine: Engine::new(dictionary),
            is_activated: false,
            show_candidates: false,
            last_candidates: Vec::new(),
        }
    }

    pub fn on_activate(&mut self) {
        self.is_activated = true;
    }

    pub fn on_deactivate(&mut self) {
        self.is_activated = false;
        self.reset_composition();
    }

    pub fn on_composition_terminated(&mut self) {
        self.reset_composition();
    }

    fn reset_composition(&mut self) {
        self.engine.clear();
        self.show_candidates = false;
        self.last_candidates.clear();
    }

    fn classify(&self, event: &KeyEventData) -> KeyAction {
        if !self.is_activated || event.has_ctrl_or_alt() {
            return KeyAction::PassThrough;
        }

        if self.show_candidates {
            if let Some(index) = digit_selection_index(event.key_code) {
                if index < self.last_candidates.len() {
                    return KeyAction::SelectCandidate(index);
                }
            }
            if event.key_code == VK_SPACE || event.key_code == VK_RETURN {
                return KeyAction::CommitTop;
            }
        }

        match event.key_code {
            VK_BACK if !self.engine.buffer().is_empty() => KeyAction::Backspace,
            VK_ESCAPE if !self.engine.buffer().is_empty() => KeyAction::Clear,
            _ => event
                .printable_char()
                .filter(|&c| self.engine.supports_key(c))
                .map_or(KeyAction::PassThrough, KeyAction::Symbol),
        }
    }

    /// 純查詢：這個按鍵按下時，這個輸入法會不會吃掉它。不改變任何狀態。
    pub fn filter_key_down(&self, event: &KeyEventData) -> bool {
        !matches!(self.classify(event), KeyAction::PassThrough)
    }

    /// 實際處理按鍵，更新組字狀態。
    pub fn on_key_down(&mut self, event: &KeyEventData) -> KeyDownOutcome {
        match self.classify(event) {
            KeyAction::Symbol(ch) => {
                let outcome = self.engine.key_press(ch);
                self.apply_engine_outcome(outcome)
            }
            KeyAction::Backspace => {
                let outcome = self.engine.backspace();
                self.apply_engine_outcome(outcome)
            }
            KeyAction::SelectCandidate(index) => self.commit_candidate(index),
            KeyAction::CommitTop => self.commit_candidate(0),
            KeyAction::Clear => {
                self.reset_composition();
                KeyDownOutcome::Cleared
            }
            KeyAction::PassThrough => KeyDownOutcome::PassThrough,
        }
    }

    fn apply_engine_outcome(&mut self, outcome: KeyOutcome) -> KeyDownOutcome {
        match outcome {
            KeyOutcome::NotHandled => KeyDownOutcome::PassThrough,
            KeyOutcome::Composing { buffer, candidates } => {
                let candidates: Vec<String> =
                    candidates.into_iter().map(|entry| entry.word).collect();
                self.show_candidates = !candidates.is_empty();
                self.last_candidates = candidates.clone();
                KeyDownOutcome::Composing {
                    buffer,
                    candidates,
                    show_candidates: self.show_candidates,
                }
            }
        }
    }

    fn commit_candidate(&mut self, index: usize) -> KeyDownOutcome {
        let Some(word) = self.last_candidates.get(index).cloned() else {
            return KeyDownOutcome::PassThrough;
        };
        let committed = self.engine.select_candidate(&word);
        self.show_candidates = false;
        self.last_candidates.clear();
        KeyDownOutcome::Committed(committed)
    }
}

fn digit_selection_index(key_code: u32) -> Option<usize> {
    if (VK_KEY_1..=VK_KEY_9).contains(&key_code) {
        Some((key_code - VK_KEY_1) as usize)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(char_code: u32, key_code: u32) -> KeyEventData {
        KeyEventData {
            char_code,
            key_code,
            key_states: Vec::new(),
        }
    }

    fn sample_dictionary() -> Dictionary {
        Dictionary::parse("ㄋㄧˇ\t你\t9000\nㄏㄠˇ\t好\t9000\nㄏㄠˇ\t號\t100\n")
    }

    #[test]
    fn inactive_session_passes_every_key_through() {
        let session = Session::new(sample_dictionary());
        assert!(!session.filter_key_down(&key('s' as u32, 0x53)));
    }

    #[test]
    fn filter_and_on_key_down_agree_on_zhuyin_keys() {
        let mut session = Session::new(sample_dictionary());
        session.on_activate();
        let s_key = key('s' as u32, 0x53);
        assert!(session.filter_key_down(&s_key));
        let outcome = session.on_key_down(&s_key);
        assert_eq!(
            outcome,
            KeyDownOutcome::Composing {
                buffer: "ㄋ".into(),
                candidates: vec![],
                show_candidates: false
            }
        );
    }

    #[test]
    fn full_composition_then_space_commits_top_candidate() {
        let mut session = Session::new(sample_dictionary());
        session.on_activate();

        // 你 = ㄋㄧˇ : s(ㄋ) u(ㄧ) 3(ˇ)
        session.on_key_down(&key('s' as u32, 0x53));
        session.on_key_down(&key('u' as u32, 0x55));
        let outcome = session.on_key_down(&key('3' as u32, 0x33));
        assert_eq!(
            outcome,
            KeyDownOutcome::Composing {
                buffer: "ㄋㄧˇ".into(),
                candidates: vec!["你".into()],
                show_candidates: true
            }
        );

        let space = key(' ' as u32, VK_SPACE);
        assert!(
            session.filter_key_down(&space),
            "候選字視窗開啟時空白鍵應被吃下"
        );
        assert_eq!(
            session.on_key_down(&space),
            KeyDownOutcome::Committed("你".into())
        );
    }

    #[test]
    fn digit_selects_candidate_only_while_candidate_window_is_open() {
        let mut session = Session::new(sample_dictionary());
        session.on_activate();

        // 好 = ㄏㄠˇ : c(ㄏ) l(ㄠ) 3(ˇ) → 候選字 ["好", "號"]
        session.on_key_down(&key('c' as u32, 0x43));
        session.on_key_down(&key('l' as u32, 0x4C));
        session.on_key_down(&key('3' as u32, 0x33));

        // 選第 2 個候選字「號」
        let digit2 = key('2' as u32, VK_KEY_1 + 1);
        assert_eq!(
            session.on_key_down(&digit2),
            KeyDownOutcome::Committed("號".into())
        );

        // 候選字視窗已關閉，'1' 應被當成注音符號（ㄅ）而非選字
        let digit1 = key('1' as u32, VK_KEY_1);
        let outcome = session.on_key_down(&digit1);
        assert_eq!(
            outcome,
            KeyDownOutcome::Composing {
                buffer: "ㄅ".into(),
                candidates: vec![],
                show_candidates: false
            }
        );
    }

    #[test]
    fn backspace_and_escape_are_noop_pass_through_when_buffer_empty() {
        let mut session = Session::new(sample_dictionary());
        session.on_activate();
        let backspace = key(0, VK_BACK);
        assert!(!session.filter_key_down(&backspace));
        assert_eq!(session.on_key_down(&backspace), KeyDownOutcome::PassThrough);
    }

    #[test]
    fn escape_clears_composition() {
        let mut session = Session::new(sample_dictionary());
        session.on_activate();
        session.on_key_down(&key('s' as u32, 0x53));
        let escape = key(0, VK_ESCAPE);
        assert!(session.filter_key_down(&escape));
        assert_eq!(session.on_key_down(&escape), KeyDownOutcome::Cleared);
    }

    #[test]
    fn ctrl_held_keys_pass_through() {
        let mut session = Session::new(sample_dictionary());
        session.on_activate();
        let mut event = key('s' as u32, 0x53);
        event.key_states = vec![0; 32];
        event.key_states[0x11] = 0x80; // Ctrl held
        assert!(!session.filter_key_down(&event));
    }

    #[test]
    fn composition_terminated_clears_state() {
        let mut session = Session::new(sample_dictionary());
        session.on_activate();
        session.on_key_down(&key('s' as u32, 0x53)); // 組字區變成 "ㄋ"
        session.on_composition_terminated();

        // 組字區已清空，接著打的介母不會接在被中止的聲母後面，而是重新起頭。
        let outcome = session.on_key_down(&key('u' as u32, 0x55));
        assert_eq!(
            outcome,
            KeyDownOutcome::Composing {
                buffer: "ㄧ".into(),
                candidates: vec![],
                show_candidates: false
            }
        );
    }
}

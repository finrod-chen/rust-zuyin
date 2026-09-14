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
//!
//! 除了注音組字，這裡也管理語言列上的兩個開關按鈕：
//!
//! - 中／英切換（`keyboard_open`，對應官方 `TextService.keyboardOpen`）：
//!   關閉時整個輸入法不攔截任何按鍵，所有輸入直接交還應用程式。
//! - 全形／半形切換（`fullwidth`）：開啟時，沒有被注音鍵盤用到的可印字元
//!   （含空白鍵，且僅限組字區是空的時候）會被轉換成全形字元後直接送出，
//!   不經過注音轉換。

use crate::protocol::KeyEventData;
use zuyin_core::{Dictionary, Engine, KeyOutcome};

const VK_BACK: u32 = 0x08;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_RETURN: u32 = 0x0D;
const VK_KEY_1: u32 = 0x31;
const VK_KEY_9: u32 = 0x39;

/// 語言列「中／英切換」按鈕的識別碼，透過 `addButton` 註冊、
/// 在 `onCommand` 中原樣收到。
pub const CHINESE_ENGLISH_BUTTON_ID: &str = "zuyin-chinese-english";
/// 語言列「全形／半形切換」按鈕的識別碼。
pub const FULLWIDTH_BUTTON_ID: &str = "zuyin-fullwidth";

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
    /// 全形模式下，非注音鍵應轉換成對應全形字元後直接送出。
    FullwidthCommit(char),
    /// 這個 IME 不處理此按鍵，應直接交還應用程式。
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
    /// 有文字要送入應用程式（選字確認、或全形字元直接送出）。
    Committed(String),
    /// 使用者按 Esc 清空組字區；組字區與候選字視窗皆應清空。
    Cleared,
    /// 未被此輸入法處理，應交還應用程式（不應設定任何組字／候選字欄位）。
    PassThrough,
}

/// 語言列按鈕目前的顯示狀態，`main.rs` 會轉成 PIME 的
/// `addButton`／`changeButton` 訊息。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonSnapshot {
    pub id: &'static str,
    pub text: &'static str,
    pub tooltip: &'static str,
    pub toggled: bool,
}

pub struct Session {
    engine: Engine,
    is_activated: bool,
    /// 中／英開關（對應官方 `TextService.keyboardOpen`）：關閉時完全不
    /// 攔截按鍵。
    keyboard_open: bool,
    /// 全形／半形開關。
    fullwidth: bool,
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
            keyboard_open: true,
            fullwidth: false,
            show_candidates: false,
            last_candidates: Vec::new(),
        }
    }

    pub fn on_activate(&mut self, is_keyboard_open: bool) {
        self.is_activated = true;
        self.keyboard_open = is_keyboard_open;
    }

    pub fn on_deactivate(&mut self) {
        self.is_activated = false;
        self.reset_composition();
    }

    pub fn on_composition_terminated(&mut self) {
        self.reset_composition();
    }

    /// 系統輸入法切換熱鍵（或其他 client）改變了中／英開關狀態。
    pub fn on_keyboard_status_changed(&mut self, opened: bool) {
        self.keyboard_open = opened;
        if !opened {
            self.reset_composition();
        }
    }

    fn reset_composition(&mut self) {
        self.engine.clear();
        self.show_candidates = false;
        self.last_candidates.clear();
    }

    /// 目前語言列兩個按鈕該顯示的狀態，供 `onActivate` 回應的
    /// `addButton` 使用。
    pub fn language_bar_buttons(&self) -> [ButtonSnapshot; 2] {
        [
            ButtonSnapshot {
                id: CHINESE_ENGLISH_BUTTON_ID,
                text: if self.keyboard_open { "中" } else { "英" },
                tooltip: "切換中文／英文輸入",
                toggled: self.keyboard_open,
            },
            ButtonSnapshot {
                id: FULLWIDTH_BUTTON_ID,
                text: if self.fullwidth { "全" } else { "半" },
                tooltip: "切換全形／半形標點與符號",
                toggled: self.fullwidth,
            },
        ]
    }

    /// 使用者點擊語言列按鈕。回傳該按鈕更新後的狀態，供 `changeButton`
    /// 使用；`id` 不是我們註冊過的按鈕則回傳 `None`（呼叫端不需回應
    /// `changeButton`）。
    pub fn on_command(&mut self, id: &str) -> Option<ButtonSnapshot> {
        match id {
            CHINESE_ENGLISH_BUTTON_ID => {
                self.keyboard_open = !self.keyboard_open;
                if !self.keyboard_open {
                    self.reset_composition();
                }
            }
            FULLWIDTH_BUTTON_ID => self.fullwidth = !self.fullwidth,
            _ => return None,
        }
        self.language_bar_buttons().into_iter().find(|b| b.id == id)
    }

    fn classify(&self, event: &KeyEventData) -> KeyAction {
        if !self.is_activated || !self.keyboard_open || event.has_ctrl_or_alt() {
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

        let buffer_empty = self.engine.buffer().is_empty();
        match event.key_code {
            VK_BACK if !buffer_empty => KeyAction::Backspace,
            VK_ESCAPE if !buffer_empty => KeyAction::Clear,
            VK_SPACE if buffer_empty && self.fullwidth => KeyAction::FullwidthCommit(' '),
            _ => {
                if let Some(ch) = event.printable_char() {
                    // Shift 是使用者「跳過注音、直接打英文」的慣例按法。
                    if !event.has_shift() && self.engine.supports_key(ch) {
                        return KeyAction::Symbol(ch);
                    }
                    if buffer_empty && self.fullwidth {
                        return KeyAction::FullwidthCommit(ch);
                    }
                }
                KeyAction::PassThrough
            }
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
            KeyAction::FullwidthCommit(ch) => {
                KeyDownOutcome::Committed(to_fullwidth(ch).to_string())
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

/// 將半形 ASCII 可印字元轉成對應的全形字元；空白鍵轉成全形空白
/// （U+3000，Unicode 對半形空白的特例，不落在 U+FF01–U+FF5E 這段
/// 「全形變體」區塊的連續偏移量規則內）。
fn to_fullwidth(ch: char) -> char {
    match ch {
        ' ' => '\u{3000}',
        '\u{21}'..='\u{7e}' => char::from_u32(ch as u32 + 0xFEE0).unwrap_or(ch),
        other => other,
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

    fn shifted_key(char_code: u32, key_code: u32) -> KeyEventData {
        let mut event = key(char_code, key_code);
        event.key_states = vec![0; 32];
        event.key_states[0x10] = 0x80; // Shift held
        event
    }

    fn sample_dictionary() -> Dictionary {
        Dictionary::parse("ㄋㄧˇ\t你\t9000\nㄏㄠˇ\t好\t9000\nㄏㄠˇ\t號\t100\n")
    }

    fn activated_session() -> Session {
        let mut session = Session::new(sample_dictionary());
        session.on_activate(true);
        session
    }

    #[test]
    fn inactive_session_passes_every_key_through() {
        let session = Session::new(sample_dictionary());
        assert!(!session.filter_key_down(&key('s' as u32, 0x53)));
    }

    #[test]
    fn filter_and_on_key_down_agree_on_zhuyin_keys() {
        let mut session = activated_session();
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
        let mut session = activated_session();

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
        let mut session = activated_session();

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
        let mut session = activated_session();
        let backspace = key(0, VK_BACK);
        assert!(!session.filter_key_down(&backspace));
        assert_eq!(session.on_key_down(&backspace), KeyDownOutcome::PassThrough);
    }

    #[test]
    fn escape_clears_composition() {
        let mut session = activated_session();
        session.on_key_down(&key('s' as u32, 0x53));
        let escape = key(0, VK_ESCAPE);
        assert!(session.filter_key_down(&escape));
        assert_eq!(session.on_key_down(&escape), KeyDownOutcome::Cleared);
    }

    #[test]
    fn ctrl_held_keys_pass_through() {
        let session = activated_session();
        let mut event = key('s' as u32, 0x53);
        event.key_states = vec![0; 32];
        event.key_states[0x11] = 0x80; // Ctrl held
        assert!(!session.filter_key_down(&event));
    }

    #[test]
    fn composition_terminated_clears_state() {
        let mut session = activated_session();
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

    #[test]
    fn keyboard_closed_passes_every_key_through_and_clears_pending_composition() {
        let mut session = activated_session();
        session.on_key_down(&key('s' as u32, 0x53)); // 組字區變成 "ㄋ"

        session.on_keyboard_status_changed(false);
        let outcome = session.on_key_down(&key('u' as u32, 0x55));
        assert_eq!(outcome, KeyDownOutcome::PassThrough);
    }

    #[test]
    fn chinese_english_button_toggles_keyboard_open() {
        let mut session = activated_session();
        assert!(
            session.language_bar_buttons()[0].toggled,
            "預設應為中文模式"
        );

        let updated = session.on_command(CHINESE_ENGLISH_BUTTON_ID).unwrap();
        assert!(!updated.toggled);
        assert!(
            !session.filter_key_down(&key('s' as u32, 0x53)),
            "切成英文後注音鍵應直接放行"
        );

        session.on_command(CHINESE_ENGLISH_BUTTON_ID);
        assert!(
            session.filter_key_down(&key('s' as u32, 0x53)),
            "切回中文後注音鍵應恢復被吃下"
        );
    }

    #[test]
    fn unknown_command_id_is_ignored() {
        let mut session = activated_session();
        assert_eq!(session.on_command("some-other-button"), None);
    }

    #[test]
    fn fullwidth_button_converts_unmapped_symbol_when_buffer_is_empty() {
        let mut session = activated_session();
        session.on_command(FULLWIDTH_BUTTON_ID);

        // '!' 不是任何注音鍵，全形模式下應直接轉換送出。
        let bang = key('!' as u32, 0x31); // key_code 隨意，非數字選字鍵、非 VK_1..VK_9 的字母鍵區
        let outcome = session.on_key_down(&bang);
        assert_eq!(outcome, KeyDownOutcome::Committed("！".into()));
    }

    #[test]
    fn fullwidth_space_only_applies_when_composition_buffer_is_empty() {
        let mut session = activated_session();
        session.on_command(FULLWIDTH_BUTTON_ID);

        session.on_key_down(&key('s' as u32, 0x53)); // 開始組字 "ㄋ"
        let space = key(' ' as u32, VK_SPACE);
        // 組字區非空，空白鍵應照原本規則處理（此情境下沒有候選字，直接放行）。
        assert_eq!(session.on_key_down(&space), KeyDownOutcome::PassThrough);

        session.on_key_down(&key(0, VK_ESCAPE)); // 清空組字區
        let outcome = session.on_key_down(&space);
        assert_eq!(outcome, KeyDownOutcome::Committed('\u{3000}'.to_string()));
    }

    #[test]
    fn shift_held_letter_bypasses_zhuyin_and_can_be_fullwidth() {
        let mut session = activated_session();
        session.on_command(FULLWIDTH_BUTTON_ID);

        // Shift+S：使用者要直接打英文字母，不應被當成注音鍵 ㄋ。
        let outcome = session.on_key_down(&shifted_key('S' as u32, 0x53));
        assert_eq!(outcome, KeyDownOutcome::Committed("Ｓ".into()));
    }

    #[test]
    fn shift_held_letter_without_fullwidth_passes_through_unconverted() {
        let mut session = activated_session();
        let outcome = session.on_key_down(&shifted_key('S' as u32, 0x53));
        assert_eq!(outcome, KeyDownOutcome::PassThrough);
    }
}

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
//! 除了注音組字，這裡也管理語言列與全域熱鍵：
//!
//! - 中／英切換（`keyboard_open`，對應官方 `TextService.keyboardOpen`）：
//!   關閉時整個輸入法不攔截任何按鍵，所有輸入直接交還應用程式。
//! - 全形／半形切換（`fullwidth`）：開啟時，沒有被注音鍵盤用到的可印字元
//!   （含空白鍵，且僅限組字區是空的時候）會被轉換成全形字元後直接送出，
//!   不經過注音轉換。也可透過 Shift+Space 這個保留鍵切換（各家注音輸入法
//!   常見的慣例快速鍵）。
//! - 「設定」是一個 `type: "menu"` 按鈕，點擊會觸發 `onMenu`，選單裡的
//!   「清除使用者選字記憶」會重置 core engine 的排序記憶。
//! - 沒有語言列圖示可看的操作（保留鍵、系統送來的中英狀態改變、清除記憶）
//!   會額外附上一則 [`MessageSnapshot`]，供 `main.rs` 轉成官方的
//!   `showMessage` 暫時提示訊息，讓使用者能看到剛剛發生了什麼。
//!
//! 語言列按鈕的字串 `id`（用於 `changeButton` 定位按鈕、`onMenu` 判斷要
//! 顯示哪個選單）與點擊一般按鈕時 `onCommand` 收到的整數 `commandId` 是
//! 兩個獨立的識別空間，詳見 `docs/PIME_PROTOCOL.md`。

use crate::protocol::KeyEventData;
use zuyin_core::{Dictionary, Engine, KeyOutcome, UserPhrases};

const VK_BACK: u32 = 0x08;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_RETURN: u32 = 0x0D;
const VK_KEY_1: u32 = 0x31;
const VK_KEY_9: u32 = 0x39;

/// `TF_MOD_SHIFT`（見 `docs/PIME_PROTOCOL.md` 的 TSF 修飾鍵常數）。
const TF_MOD_SHIFT: u32 = 0x0004;

/// 語言列「中／英切換」按鈕的字串識別碼。
pub const CHINESE_ENGLISH_BUTTON_ID: &str = "zuyin-chinese-english";
/// 語言列「全形／半形切換」按鈕的字串識別碼。
pub const FULLWIDTH_BUTTON_ID: &str = "zuyin-fullwidth";
/// 語言列「設定」選單按鈕的字串識別碼。
pub const SETTINGS_BUTTON_ID: &str = "zuyin-settings";

/// 中／英切換按鈕點擊時，`onCommand` 收到的 `commandId`。
pub const COMMAND_CHINESE_ENGLISH: i64 = 1;
/// 全形／半形切換按鈕（或設定選單裡的同功能項目）點擊時的 `commandId`。
pub const COMMAND_FULLWIDTH: i64 = 2;
/// 設定選單裡「清除使用者選字記憶」項目的 `commandId`。
pub const COMMAND_CLEAR_MEMORY: i64 = 3;

/// Shift+Space 保留鍵的 GUID（純粹是本專案內部的固定識別碼，不需要向
/// 任何外部單位登記）。
pub const SHIFT_SPACE_PRESERVED_KEY_GUID: &str = "{9dbf7b72-a1f5-4e00-9e7a-3b1b7a2c0f01}";

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
        /// 這個按鍵若觸發了 core engine 的貪婪最長匹配自動收斂（見
        /// `zuyin_core` 文件），這裡是依序自動送出、已接在一起的文字；
        /// 絕大多數情況下是空字串。呼叫端應把它當成跟這次組字狀態同時
        /// 發生、但先於組字區內容的既定輸出。
        flushed: String,
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

/// 按鈕點擊後的行為分類：一般按鈕點擊觸發 `onCommand`（帶固定的
/// `command_id`）；選單按鈕點擊觸發 `onMenu`（用按鈕自己的字串 `id`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Toggle { command_id: i64, toggled: bool },
    Menu,
}

/// 語言列按鈕目前的顯示狀態，`main.rs` 會轉成 PIME 的
/// `addButton`／`changeButton` 訊息。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonSnapshot {
    pub id: &'static str,
    pub text: &'static str,
    pub tooltip: &'static str,
    pub kind: ButtonKind,
}

/// 語言列選單的一個項目；`main.rs` 會轉成 PIME 的 `onMenu` 回應格式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Separator,
    Item {
        text: &'static str,
        command_id: i64,
    },
    CheckableItem {
        text: &'static str,
        command_id: i64,
        checked: bool,
    },
}

/// 保留鍵註冊資訊；`main.rs` 會轉成 PIME 的 `addPreservedKey` 訊息。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreservedKeySnapshot {
    pub key_code: u32,
    pub modifiers: u32,
    pub guid: &'static str,
}

/// 這個輸入法要求 PIME 註冊的全部保留鍵；與個別 session 狀態無關，
/// `onActivate` 時原樣回傳即可。
pub fn preserved_keys() -> [PreservedKeySnapshot; 1] {
    [PreservedKeySnapshot {
        key_code: VK_SPACE,
        modifiers: TF_MOD_SHIFT,
        guid: SHIFT_SPACE_PRESERVED_KEY_GUID,
    }]
}

/// 候選字視窗外觀設定；與個別 session 狀態無關，`onActivate` 時原樣回傳
/// 即可。`use_cursor: false` 對應「用數字鍵 1-9 選字」，跟本專案唯一
/// 支援的選字方式一致（見 [`Session::classify`] 的候選字視窗處理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateUiSnapshot {
    pub font_name: &'static str,
    pub font_size: u32,
    pub candidates_per_row: u32,
    pub use_cursor: bool,
}

pub fn candidate_ui() -> CandidateUiSnapshot {
    CandidateUiSnapshot {
        font_name: "微軟正黑體",
        font_size: 16,
        candidates_per_row: 10,
        use_cursor: false,
    }
}

/// 暫時提示訊息，`main.rs` 會轉成 PIME 的 `showMessage`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageSnapshot {
    pub text: &'static str,
    pub duration_secs: u32,
}

/// 處理語言列相關事件（按鈕、選單項目、保留鍵、系統中英狀態通知）後，
/// 需要回饋給使用者的畫面更新。兩個欄位互相獨立，皆可能為 `None`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiUpdate {
    /// 外觀改變的按鈕，供 `changeButton` 使用。
    pub button: Option<ButtonSnapshot>,
    /// 要顯示的暫時提示訊息，供 `showMessage` 使用。
    pub message: Option<MessageSnapshot>,
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

    /// 掛上使用者自訂詞庫（見 `zuyin_core` 的 `user_phrases` 模組文件），
    /// 讓打自訂注音碼時能優先叫出使用者自己設定的地址／姓名／電話等
    /// 捷徑文字。
    pub fn set_user_phrases(&mut self, user_phrases: UserPhrases) {
        self.engine.set_user_phrases(user_phrases);
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

    /// 系統輸入法切換熱鍵（或其他 client）改變了中／英開關狀態。這個改變
    /// 不是使用者點擊本輸入法的按鈕造成的，語言列圖示可能不在使用者視線
    /// 範圍內，因此額外回傳一則提示訊息。
    pub fn on_keyboard_status_changed(&mut self, opened: bool) -> UiUpdate {
        self.keyboard_open = opened;
        if !opened {
            self.reset_composition();
        }
        UiUpdate {
            button: self.button_snapshot(CHINESE_ENGLISH_BUTTON_ID),
            message: Some(MessageSnapshot {
                text: if opened {
                    "中文模式"
                } else {
                    "英文模式"
                },
                duration_secs: 2,
            }),
        }
    }

    fn reset_composition(&mut self) {
        self.engine.clear();
        self.show_candidates = false;
        self.last_candidates.clear();
    }

    /// 目前語言列三個按鈕該顯示的狀態，供 `onActivate` 回應的
    /// `addButton` 使用。
    pub fn language_bar_buttons(&self) -> [ButtonSnapshot; 3] {
        [
            ButtonSnapshot {
                id: CHINESE_ENGLISH_BUTTON_ID,
                text: if self.keyboard_open { "中" } else { "英" },
                tooltip: "切換中文／英文輸入",
                kind: ButtonKind::Toggle {
                    command_id: COMMAND_CHINESE_ENGLISH,
                    toggled: self.keyboard_open,
                },
            },
            ButtonSnapshot {
                id: FULLWIDTH_BUTTON_ID,
                text: if self.fullwidth { "全" } else { "半" },
                tooltip: "切換全形／半形標點與符號",
                kind: ButtonKind::Toggle {
                    command_id: COMMAND_FULLWIDTH,
                    toggled: self.fullwidth,
                },
            },
            ButtonSnapshot {
                id: SETTINGS_BUTTON_ID,
                text: "設定",
                tooltip: "設定",
                kind: ButtonKind::Menu,
            },
        ]
    }

    /// 使用者點擊了帶 `commandId` 的按鈕，或從 `onMenu` 選單選了一個
    /// 項目（兩者共用同一組 `command_id`）。切換按鈕本身已有圖示變化當
    /// 作視覺回饋，不額外顯示提示訊息；沒有圖示可看的動作（例如清除
    /// 記憶）才透過 `message` 回饋結果。未知的 `command_id` 回傳
    /// `UiUpdate::default()`（兩個欄位皆為 `None`）。
    pub fn on_command(&mut self, command_id: i64) -> UiUpdate {
        match command_id {
            COMMAND_CHINESE_ENGLISH => {
                self.keyboard_open = !self.keyboard_open;
                if !self.keyboard_open {
                    self.reset_composition();
                }
                UiUpdate {
                    button: self.button_snapshot(CHINESE_ENGLISH_BUTTON_ID),
                    message: None,
                }
            }
            COMMAND_FULLWIDTH => {
                self.fullwidth = !self.fullwidth;
                UiUpdate {
                    button: self.button_snapshot(FULLWIDTH_BUTTON_ID),
                    message: None,
                }
            }
            COMMAND_CLEAR_MEMORY => {
                self.engine.forget_selections();
                UiUpdate {
                    button: None,
                    message: Some(MessageSnapshot {
                        text: "已清除使用者選字記憶",
                        duration_secs: 2,
                    }),
                }
            }
            _ => UiUpdate::default(),
        }
    }

    /// 使用者點擊 `type: "menu"` 的按鈕。目前只有「設定」按鈕有選單。
    pub fn on_menu(&self, button_id: &str) -> Option<Vec<MenuEntry>> {
        if button_id != SETTINGS_BUTTON_ID {
            return None;
        }
        Some(vec![
            MenuEntry::CheckableItem {
                text: "全形／半形輸入 (&F)",
                command_id: COMMAND_FULLWIDTH,
                checked: self.fullwidth,
            },
            MenuEntry::Separator,
            MenuEntry::Item {
                text: "清除使用者選字記憶 (&C)",
                command_id: COMMAND_CLEAR_MEMORY,
            },
        ])
    }

    /// 使用者按下透過 [`preserved_keys`] 註冊的全域熱鍵組合。回傳
    /// `true` 代表這個 guid 有對應行為並已處理。保留鍵沒有按鈕點擊那樣
    /// 直接的視覺回饋，所以額外附上提示訊息。
    pub fn on_preserved_key(&mut self, guid: &str) -> (bool, UiUpdate) {
        if guid != SHIFT_SPACE_PRESERVED_KEY_GUID {
            return (false, UiUpdate::default());
        }
        self.fullwidth = !self.fullwidth;
        let update = UiUpdate {
            button: self.button_snapshot(FULLWIDTH_BUTTON_ID),
            message: Some(MessageSnapshot {
                text: if self.fullwidth {
                    "已切換為全形"
                } else {
                    "已切換為半形"
                },
                duration_secs: 2,
            }),
        };
        (true, update)
    }

    fn button_snapshot(&self, id: &str) -> Option<ButtonSnapshot> {
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
            KeyOutcome::Composing {
                flushed,
                buffer,
                candidates,
            } => {
                let candidates: Vec<String> =
                    candidates.into_iter().map(|entry| entry.word).collect();
                self.show_candidates = !candidates.is_empty();
                self.last_candidates = candidates.clone();
                KeyDownOutcome::Composing {
                    flushed: flushed.join(""),
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
                flushed: String::new(),
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
                flushed: String::new(),
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
                flushed: String::new(),
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
                flushed: String::new(),
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

        let update = session.on_keyboard_status_changed(false);
        assert_eq!(
            update.message,
            Some(MessageSnapshot {
                text: "英文模式",
                duration_secs: 2
            })
        );
        let outcome = session.on_key_down(&key('u' as u32, 0x55));
        assert_eq!(outcome, KeyDownOutcome::PassThrough);
    }

    #[test]
    fn keyboard_status_changed_message_reflects_chinese_mode() {
        let mut session = activated_session();
        let update = session.on_keyboard_status_changed(true);
        assert_eq!(
            update.message,
            Some(MessageSnapshot {
                text: "中文模式",
                duration_secs: 2
            })
        );
    }

    fn toggled(snapshot: ButtonSnapshot) -> bool {
        match snapshot.kind {
            ButtonKind::Toggle { toggled, .. } => toggled,
            ButtonKind::Menu => panic!("{} is not a toggle button", snapshot.id),
        }
    }

    #[test]
    fn chinese_english_button_toggles_keyboard_open() {
        let mut session = activated_session();
        assert!(
            toggled(session.language_bar_buttons()[0]),
            "預設應為中文模式"
        );

        let update = session.on_command(COMMAND_CHINESE_ENGLISH);
        assert!(!toggled(update.button.unwrap()));
        assert_eq!(
            update.message, None,
            "按鈕點擊已有圖示變化，不需要額外提示訊息"
        );
        assert!(
            !session.filter_key_down(&key('s' as u32, 0x53)),
            "切成英文後注音鍵應直接放行"
        );

        session.on_command(COMMAND_CHINESE_ENGLISH);
        assert!(
            session.filter_key_down(&key('s' as u32, 0x53)),
            "切回中文後注音鍵應恢復被吃下"
        );
    }

    #[test]
    fn unknown_command_id_is_ignored() {
        let mut session = activated_session();
        assert_eq!(session.on_command(999), UiUpdate::default());
    }

    #[test]
    fn clear_memory_command_shows_message_but_changes_no_button() {
        let mut session = activated_session();
        let update = session.on_command(COMMAND_CLEAR_MEMORY);
        assert_eq!(update.button, None);
        assert_eq!(
            update.message,
            Some(MessageSnapshot {
                text: "已清除使用者選字記憶",
                duration_secs: 2
            })
        );
    }

    #[test]
    fn settings_button_menu_reflects_fullwidth_state() {
        let mut session = activated_session();
        let menu = session
            .on_menu(SETTINGS_BUTTON_ID)
            .expect("設定按鈕應有選單");
        assert_eq!(
            menu,
            vec![
                MenuEntry::CheckableItem {
                    text: "全形／半形輸入 (&F)",
                    command_id: COMMAND_FULLWIDTH,
                    checked: false,
                },
                MenuEntry::Separator,
                MenuEntry::Item {
                    text: "清除使用者選字記憶 (&C)",
                    command_id: COMMAND_CLEAR_MEMORY
                },
            ]
        );

        session.on_command(COMMAND_FULLWIDTH);
        let menu = session.on_menu(SETTINGS_BUTTON_ID).unwrap();
        assert_eq!(
            menu[0],
            MenuEntry::CheckableItem {
                text: "全形／半形輸入 (&F)",
                command_id: COMMAND_FULLWIDTH,
                checked: true
            }
        );
    }

    #[test]
    fn non_settings_button_has_no_menu() {
        let session = activated_session();
        assert_eq!(session.on_menu(CHINESE_ENGLISH_BUTTON_ID), None);
    }

    #[test]
    fn clear_memory_command_resets_learned_ranking() {
        let dict = Dictionary::parse("ㄏㄠˇ\t好\t250\nㄏㄠˇ\t號\t100\n");
        let mut session = Session::new(dict);
        session.on_activate(true);

        // 使用者兩次都選詞頻較低的「號」，累積加權超過「好」的基礎詞頻差距。
        for _ in 0..2 {
            session.on_key_down(&key('c' as u32, 0x43));
            session.on_key_down(&key('l' as u32, 0x4C));
            session.on_key_down(&key('3' as u32, 0x33));
            session.on_key_down(&key('2' as u32, VK_KEY_1 + 1)); // 選第 2 個候選字「號」
        }

        session.on_command(COMMAND_CLEAR_MEMORY);

        session.on_key_down(&key('c' as u32, 0x43));
        session.on_key_down(&key('l' as u32, 0x4C));
        let outcome = session.on_key_down(&key('3' as u32, 0x33));
        match outcome {
            KeyDownOutcome::Composing { candidates, .. } => {
                assert_eq!(candidates[0], "好", "清除記憶後應退回純詞頻排序");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn shift_space_preserved_key_toggles_fullwidth() {
        let mut session = activated_session();
        let (handled, update) = session.on_preserved_key(SHIFT_SPACE_PRESERVED_KEY_GUID);
        assert!(handled);
        assert!(toggled(update.button.unwrap()), "應切成全形");
        assert_eq!(
            update.message,
            Some(MessageSnapshot {
                text: "已切換為全形",
                duration_secs: 2
            })
        );

        // '!' 不是任何注音鍵，全形模式下應直接轉換送出。
        let outcome = session.on_key_down(&key('!' as u32, 0x31));
        assert_eq!(outcome, KeyDownOutcome::Committed("！".into()));

        // 再按一次應切回半形，訊息文字也要跟著換。
        let (handled, update) = session.on_preserved_key(SHIFT_SPACE_PRESERVED_KEY_GUID);
        assert!(handled);
        assert!(!toggled(update.button.unwrap()));
        assert_eq!(
            update.message,
            Some(MessageSnapshot {
                text: "已切換為半形",
                duration_secs: 2
            })
        );
    }

    #[test]
    fn unknown_preserved_key_guid_is_not_handled() {
        let mut session = activated_session();
        let (handled, update) = session.on_preserved_key("{00000000-0000-0000-0000-000000000000}");
        assert!(!handled);
        assert_eq!(update, UiUpdate::default());
    }

    #[test]
    fn candidate_ui_uses_number_key_selection() {
        // use_cursor 必須是 false：本專案只支援用數字鍵 1-9 選字，
        // 沒有實作游標／方向鍵選字。
        assert!(!candidate_ui().use_cursor);
    }

    #[test]
    fn fullwidth_button_converts_unmapped_symbol_when_buffer_is_empty() {
        let mut session = activated_session();
        session.on_command(COMMAND_FULLWIDTH);

        // '!' 不是任何注音鍵，全形模式下應直接轉換送出。
        let bang = key('!' as u32, 0x31); // key_code 隨意，非數字選字鍵、非 VK_1..VK_9 的字母鍵區
        let outcome = session.on_key_down(&bang);
        assert_eq!(outcome, KeyDownOutcome::Committed("！".into()));
    }

    #[test]
    fn fullwidth_space_only_applies_when_composition_buffer_is_empty() {
        let mut session = activated_session();
        session.on_command(COMMAND_FULLWIDTH);

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
        session.on_command(COMMAND_FULLWIDTH);

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

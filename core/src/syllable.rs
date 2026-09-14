//! 音節狀態機：依「聲母 → 介母 → 韻母 → 聲調」的順序組合注音符號，
//! 並驗證按鍵組合是否合法。

use crate::keyboard::{SymbolKind, ZhuyinSymbol};

/// 送入符號後的處理結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushResult {
    /// 符號已接受，音節仍在組合中。
    Accepted,
    /// 該類別的符號已經填過（例如重複輸入聲母），符號未被接受。
    Rejected,
}

/// 單一注音音節的組字狀態。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Syllable {
    initial: Option<char>,
    medial: Option<char>,
    r#final: Option<char>,
    tone: Option<char>,
}

impl Syllable {
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否尚未輸入任何符號。
    pub fn is_empty(&self) -> bool {
        self.initial.is_none()
            && self.medial.is_none()
            && self.r#final.is_none()
            && self.tone.is_none()
    }

    /// 是否已組成可查詢詞庫的音節（至少要有聲母／介母／韻母其一；
    /// 單獨的聲調不足以構成音節）。
    pub fn is_ready(&self) -> bool {
        self.initial.is_some() || self.medial.is_some() || self.r#final.is_some()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// 送入一個已由鍵盤佈局解析出的注音符號。
    ///
    /// 同一類別的符號只能出現一次；重複送入會被拒絕，呼叫端可依此判斷
    /// 是否該結束目前音節、開始下一個音節。
    pub fn push(&mut self, symbol: ZhuyinSymbol) -> PushResult {
        let slot = match symbol.kind {
            SymbolKind::Initial => &mut self.initial,
            SymbolKind::Medial => &mut self.medial,
            SymbolKind::Final => &mut self.r#final,
            SymbolKind::Tone => &mut self.tone,
        };
        if slot.is_some() {
            return PushResult::Rejected;
        }
        *slot = Some(symbol.glyph);
        PushResult::Accepted
    }

    /// 依「聲調 → 韻母 → 介母 → 聲母」的順序刪除最後輸入的符號。
    pub fn backspace(&mut self) {
        if self.tone.take().is_some() {
            return;
        }
        if self.r#final.take().is_some() {
            return;
        }
        if self.medial.take().is_some() {
            return;
        }
        self.initial.take();
    }

    /// 組成目前的注音字串，供組字區顯示與詞庫查詢使用。
    pub fn as_zhuyin_string(&self) -> String {
        [self.initial, self.medial, self.r#final, self.tone]
            .into_iter()
            .flatten()
            .collect()
    }

    /// 這個音節「第一個打的符號」（依聲母 → 介母 → 韻母的順序，取第一個
    /// 已填的），也就是 [`Syllable::as_zhuyin_string`] 結果的第一個字元；
    /// 用於「注音縮寫輸入」——只打每個字的第一個符號就能查詢對應候選字
    /// （見 [`crate::dictionary::Dictionary::lookup_abbreviation`]）。
    /// 完全沒填任何符號時回傳 `None`。
    pub fn leading_glyph(&self) -> Option<char> {
        self.initial.or(self.medial).or(self.r#final)
    }

    /// 是否已經輸入聲調符號。
    pub fn has_tone(&self) -> bool {
        self.tone.is_some()
    }

    /// 只由聲母／介母／韻母組成的注音字串（不含聲調），也就是
    /// [`Syllable::as_zhuyin_string`] 拿掉聲調的版本；用於「不分聲調
    /// 選字」——使用者只打完拼讀符號、還沒（或不想）指定聲調，也能查到
    /// 這個音節所有聲調的候選字（見
    /// [`crate::dictionary::Dictionary::lookup_toneless`]）。
    pub fn base_zhuyin_string(&self) -> String {
        [self.initial, self.medial, self.r#final]
            .into_iter()
            .flatten()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::StandardLayout;

    fn push_keys(syllable: &mut Syllable, keys: &str) {
        let layout = StandardLayout;
        for key in keys.chars() {
            let symbol = layout
                .lookup(key)
                .expect("test key must be a valid zhuyin key");
            syllable.push(symbol);
        }
    }

    #[test]
    fn composes_ni3_from_keys() {
        // 你 = ㄋㄧˇ : s(ㄋ) u(ㄧ) 3(ˇ)
        let mut syllable = Syllable::new();
        push_keys(&mut syllable, "su3");
        assert_eq!(syllable.as_zhuyin_string(), "ㄋㄧˇ");
        assert!(syllable.is_ready());
    }

    #[test]
    fn tone_only_is_not_ready() {
        let mut syllable = Syllable::new();
        push_keys(&mut syllable, "3");
        assert!(!syllable.is_ready());
    }

    #[test]
    fn duplicate_symbol_kind_is_rejected() {
        let layout = StandardLayout;
        let mut syllable = Syllable::new();
        assert_eq!(
            syllable.push(layout.lookup('s').unwrap()),
            PushResult::Accepted
        );
        // 'd' is also an Initial (ㄎ) — 聲母已填過，應被拒絕
        assert_eq!(
            syllable.push(layout.lookup('d').unwrap()),
            PushResult::Rejected
        );
        assert_eq!(syllable.as_zhuyin_string(), "ㄋ");
    }

    #[test]
    fn backspace_removes_in_reverse_order() {
        let mut syllable = Syllable::new();
        push_keys(&mut syllable, "su3");
        syllable.backspace();
        assert_eq!(syllable.as_zhuyin_string(), "ㄋㄧ");
        syllable.backspace();
        assert_eq!(syllable.as_zhuyin_string(), "ㄋ");
        syllable.backspace();
        assert!(syllable.is_empty());
    }

    #[test]
    fn clear_resets_state() {
        let mut syllable = Syllable::new();
        push_keys(&mut syllable, "su3");
        syllable.clear();
        assert!(syllable.is_empty());
        assert_eq!(syllable.as_zhuyin_string(), "");
    }

    #[test]
    fn leading_glyph_is_the_initial_when_present() {
        let mut syllable = Syllable::new();
        push_keys(&mut syllable, "s"); // ㄋ
        assert_eq!(syllable.leading_glyph(), Some('ㄋ'));
    }

    #[test]
    fn leading_glyph_falls_back_to_medial_or_final_without_an_initial() {
        let mut syllable = Syllable::new();
        push_keys(&mut syllable, "j"); // 灣 = ㄨㄢ 的 ㄨ（介母），沒有聲母
        assert_eq!(syllable.leading_glyph(), Some('ㄨ'));
    }

    #[test]
    fn leading_glyph_is_none_for_an_empty_syllable() {
        let syllable = Syllable::new();
        assert_eq!(syllable.leading_glyph(), None);
    }

    #[test]
    fn leading_glyph_matches_the_first_character_of_the_full_string() {
        let mut syllable = Syllable::new();
        push_keys(&mut syllable, "su3");
        assert_eq!(
            syllable.leading_glyph(),
            syllable.as_zhuyin_string().chars().next()
        );
    }

    #[test]
    fn base_zhuyin_string_drops_the_tone() {
        let mut syllable = Syllable::new();
        push_keys(&mut syllable, "su3");
        assert_eq!(syllable.as_zhuyin_string(), "ㄋㄧˇ");
        assert_eq!(syllable.base_zhuyin_string(), "ㄋㄧ");
        assert!(syllable.has_tone());
    }

    #[test]
    fn base_zhuyin_string_is_unchanged_when_no_tone_was_typed() {
        let mut syllable = Syllable::new();
        push_keys(&mut syllable, "su");
        assert_eq!(syllable.base_zhuyin_string(), "ㄋㄧ");
        assert_eq!(syllable.base_zhuyin_string(), syllable.as_zhuyin_string());
        assert!(!syllable.has_tone());
    }
}

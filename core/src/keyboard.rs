//! 注音鍵盤佈局定義。
//!
//! 實作 Windows 上兩種最常用的注音鍵盤佈局：[`StandardLayout`]（大千式，
//! 各平台預設值）與 [`EtenLayout`]（倚天式）。IBM／精業／許氏配置實務上
//! 較少人用，先不實作（見 `docs/PROJECT_PLAN.md` Phase 1 規劃）。兩份鍵位
//! 對照表都是照抄 [libchewing](https://github.com/chewing/libchewing) 的
//! 權威實作核對過的（`src/editor/zhuyin_layout/standard.rs` 與
//! `.../et.rs`），不是憑印象自己排的。

use std::fmt;

/// 注音符號在音節中扮演的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolKind {
    /// 聲母，例如 ㄅㄆㄇㄈ
    Initial,
    /// 介母，例如 ㄧㄨㄩ
    Medial,
    /// 韻母，例如 ㄚㄛㄜㄝ
    Final,
    /// 聲調，例如 ˊˇˋ˙（第一聲無符號，不會出現在此列舉中）
    Tone,
}

/// 單一注音符號，包含其字符與角色分類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ZhuyinSymbol {
    pub kind: SymbolKind,
    pub glyph: char,
}

impl fmt::Display for ZhuyinSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.glyph)
    }
}

macro_rules! symbol {
    ($kind:ident, $glyph:expr) => {
        ZhuyinSymbol {
            kind: SymbolKind::$kind,
            glyph: $glyph,
        }
    };
}

/// 四個聲調符號（第一聲無符號，不列在這裡）。在任何注音字串裡，聲調
/// 永遠是最後一個字元（見 [`crate::syllable::Syllable::as_zhuyin_string`]
/// 的組成順序），所以只要檢查字串最後一個字元是否屬於這個集合，就能判斷
/// 一個音節字串有沒有指定聲調（見
/// [`crate::dictionary::Dictionary::lookup_toneless`]）。
pub const TONE_MARKS: [char; 4] = ['ˊ', 'ˇ', 'ˋ', '˙'];

/// 使用者可選擇的注音鍵盤佈局（見模組文件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardLayout {
    /// 大千式，各平台預設值。
    Standard(StandardLayout),
    /// 倚天式。
    Eten(EtenLayout),
}

impl Default for KeyboardLayout {
    fn default() -> Self {
        KeyboardLayout::Standard(StandardLayout)
    }
}

impl KeyboardLayout {
    /// 查詢按鍵對應的注音符號；非注音鍵回傳 `None`。
    pub fn lookup(&self, key: char) -> Option<ZhuyinSymbol> {
        match self {
            KeyboardLayout::Standard(layout) => layout.lookup(key),
            KeyboardLayout::Eten(layout) => layout.lookup(key),
        }
    }
}

/// 大千式（Windows 內建標準）注音鍵盤佈局：將 QWERTY 按鍵對應到注音符號。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StandardLayout;

impl StandardLayout {
    /// 查詢按鍵對應的注音符號；非注音鍵回傳 `None`。
    pub fn lookup(&self, key: char) -> Option<ZhuyinSymbol> {
        let key = key.to_ascii_lowercase();
        Some(match key {
            '1' => symbol!(Initial, 'ㄅ'),
            '2' => symbol!(Initial, 'ㄉ'),
            '3' => symbol!(Tone, 'ˇ'),
            '4' => symbol!(Tone, 'ˋ'),
            '5' => symbol!(Initial, 'ㄓ'),
            '6' => symbol!(Tone, 'ˊ'),
            '7' => symbol!(Tone, '˙'),
            '8' => symbol!(Final, 'ㄚ'),
            '9' => symbol!(Final, 'ㄞ'),
            '0' => symbol!(Final, 'ㄢ'),
            '-' => symbol!(Final, 'ㄦ'),

            'q' => symbol!(Initial, 'ㄆ'),
            'w' => symbol!(Initial, 'ㄊ'),
            'e' => symbol!(Initial, 'ㄍ'),
            'r' => symbol!(Initial, 'ㄐ'),
            't' => symbol!(Initial, 'ㄔ'),
            'y' => symbol!(Initial, 'ㄗ'),
            'u' => symbol!(Medial, 'ㄧ'),
            'i' => symbol!(Final, 'ㄛ'),
            'o' => symbol!(Final, 'ㄟ'),
            'p' => symbol!(Final, 'ㄣ'),

            'a' => symbol!(Initial, 'ㄇ'),
            's' => symbol!(Initial, 'ㄋ'),
            'd' => symbol!(Initial, 'ㄎ'),
            'f' => symbol!(Initial, 'ㄑ'),
            'g' => symbol!(Initial, 'ㄕ'),
            'h' => symbol!(Initial, 'ㄘ'),
            'j' => symbol!(Medial, 'ㄨ'),
            'k' => symbol!(Final, 'ㄜ'),
            'l' => symbol!(Final, 'ㄠ'),
            ';' => symbol!(Final, 'ㄤ'),

            'z' => symbol!(Initial, 'ㄈ'),
            'x' => symbol!(Initial, 'ㄌ'),
            'c' => symbol!(Initial, 'ㄏ'),
            'v' => symbol!(Initial, 'ㄒ'),
            'b' => symbol!(Initial, 'ㄖ'),
            'n' => symbol!(Initial, 'ㄙ'),
            'm' => symbol!(Medial, 'ㄩ'),
            ',' => symbol!(Final, 'ㄝ'),
            '.' => symbol!(Final, 'ㄡ'),
            '/' => symbol!(Final, 'ㄥ'),

            _ => return None,
        })
    }
}

/// 倚天式（ET41）注音鍵盤佈局：另一種在 Windows 上常見的鍵盤佈局，跟
/// 大千式鍵位完全不同（見模組文件）。跟大千式不同，倚天式用到了 `=` 跟
/// `'`（單引號）這兩個大千式沒用到的鍵位。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EtenLayout;

impl EtenLayout {
    /// 查詢按鍵對應的注音符號；非注音鍵回傳 `None`。
    pub fn lookup(&self, key: char) -> Option<ZhuyinSymbol> {
        let key = key.to_ascii_lowercase();
        Some(match key {
            '1' => symbol!(Tone, '˙'),
            '2' => symbol!(Tone, 'ˊ'),
            '3' => symbol!(Tone, 'ˇ'),
            '4' => symbol!(Tone, 'ˋ'),
            '7' => symbol!(Initial, 'ㄑ'),
            '8' => symbol!(Final, 'ㄢ'),
            '9' => symbol!(Final, 'ㄣ'),
            '0' => symbol!(Final, 'ㄤ'),
            '-' => symbol!(Final, 'ㄥ'),
            '=' => symbol!(Final, 'ㄦ'),

            'q' => symbol!(Final, 'ㄟ'),
            'w' => symbol!(Final, 'ㄝ'),
            'e' => symbol!(Medial, 'ㄧ'),
            'r' => symbol!(Final, 'ㄜ'),
            't' => symbol!(Initial, 'ㄊ'),
            'y' => symbol!(Final, 'ㄡ'),
            'u' => symbol!(Medial, 'ㄩ'),
            'i' => symbol!(Final, 'ㄞ'),
            'o' => symbol!(Final, 'ㄛ'),
            'p' => symbol!(Initial, 'ㄆ'),

            'a' => symbol!(Final, 'ㄚ'),
            's' => symbol!(Initial, 'ㄙ'),
            'd' => symbol!(Initial, 'ㄉ'),
            'f' => symbol!(Initial, 'ㄈ'),
            'g' => symbol!(Initial, 'ㄐ'),
            'h' => symbol!(Initial, 'ㄏ'),
            'j' => symbol!(Initial, 'ㄖ'),
            'k' => symbol!(Initial, 'ㄎ'),
            'l' => symbol!(Initial, 'ㄌ'),
            ';' => symbol!(Initial, 'ㄗ'),
            '\'' => symbol!(Initial, 'ㄘ'),

            'z' => symbol!(Final, 'ㄠ'),
            'x' => symbol!(Medial, 'ㄨ'),
            'c' => symbol!(Initial, 'ㄒ'),
            'v' => symbol!(Initial, 'ㄍ'),
            'b' => symbol!(Initial, 'ㄅ'),
            'n' => symbol!(Initial, 'ㄋ'),
            'm' => symbol!(Initial, 'ㄇ'),
            ',' => symbol!(Initial, 'ㄓ'),
            '.' => symbol!(Initial, 'ㄔ'),
            '/' => symbol!(Initial, 'ㄕ'),

            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_37_zhuyin_letters_and_4_tone_marks() {
        let layout = StandardLayout;
        let mapped: std::collections::HashSet<char> = "1234567890-qwertyuiopasdfghjkl;zxcvbnm,./"
            .chars()
            .filter_map(|k| layout.lookup(k))
            .map(|s| s.glyph)
            .collect();
        assert_eq!(mapped.len(), 41, "37 注音字母 + 4 聲調符號 = 41");
    }

    #[test]
    fn unmapped_key_returns_none() {
        let layout = StandardLayout;
        assert_eq!(layout.lookup('!'), None);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let layout = StandardLayout;
        assert_eq!(layout.lookup('s'), layout.lookup('S'));
    }

    #[test]
    fn known_mappings() {
        let layout = StandardLayout;
        assert_eq!(layout.lookup('s').unwrap().glyph, 'ㄋ');
        assert_eq!(layout.lookup('u').unwrap().glyph, 'ㄧ');
        assert_eq!(layout.lookup('3').unwrap().glyph, 'ˇ');
        assert_eq!(layout.lookup('s').unwrap().kind, SymbolKind::Initial);
        assert_eq!(layout.lookup('u').unwrap().kind, SymbolKind::Medial);
        assert_eq!(layout.lookup('3').unwrap().kind, SymbolKind::Tone);
    }

    #[test]
    fn h_and_b_match_the_real_dai_chien_layout() {
        // 這兩個鍵先前寫反了（h 誤植成 ㄖ、b 誤植成 ㄘ）；對照
        // libchewing `src/editor/zhuyin_layout/standard.rs` 權威實作
        // 修正回來：h 是 ㄘ、b 是 ㄖ。
        let layout = StandardLayout;
        assert_eq!(layout.lookup('h').unwrap().glyph, 'ㄘ');
        assert_eq!(layout.lookup('b').unwrap().glyph, 'ㄖ');
    }

    #[test]
    fn eten_layout_maps_all_37_zhuyin_letters_and_4_tone_marks() {
        let layout = EtenLayout;
        let mapped: std::collections::HashSet<char> = "1234567890-=qwertyuiopasdfghjkl;'zxcvbnm,./"
            .chars()
            .filter_map(|k| layout.lookup(k))
            .map(|s| s.glyph)
            .collect();
        assert_eq!(mapped.len(), 41, "37 注音字母 + 4 聲調符號 = 41");
    }

    #[test]
    fn eten_known_mappings() {
        // 對照 libchewing `src/editor/zhuyin_layout/et.rs` 權威實作。
        let layout = EtenLayout;
        assert_eq!(layout.lookup('b').unwrap().glyph, 'ㄅ'); // 大千式是 q
        assert_eq!(layout.lookup('e').unwrap().glyph, 'ㄧ'); // 大千式是 u
        assert_eq!(layout.lookup('e').unwrap().kind, SymbolKind::Medial);
        assert_eq!(layout.lookup('1').unwrap().glyph, '˙'); // 大千式是 7
        assert_eq!(layout.lookup('\'').unwrap().glyph, 'ㄘ');
        assert_eq!(layout.lookup('=').unwrap().glyph, 'ㄦ'); // 大千式是 -
    }

    #[test]
    fn keyboard_layout_enum_dispatches_to_the_selected_layout() {
        let standard = KeyboardLayout::Standard(StandardLayout);
        let eten = KeyboardLayout::Eten(EtenLayout);
        assert_eq!(standard.lookup('s').unwrap().glyph, 'ㄋ');
        assert_eq!(eten.lookup('s').unwrap().glyph, 'ㄙ');
        assert_eq!(KeyboardLayout::default(), standard, "預設應該是大千式");
    }
}

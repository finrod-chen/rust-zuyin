//! 注音鍵盤佈局定義。
//!
//! 目前僅實作「大千式」（Windows 內建標準注音鍵盤）佈局。其餘佈局
//! （倚天／IBM／精業／許氏）留待後續依需求擴充，詳見 `docs/PROJECT_PLAN.md`
//! Phase 1 規劃。

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

/// 大千式（Windows 內建標準）注音鍵盤佈局：將 QWERTY 按鍵對應到注音符號。
#[derive(Debug, Default, Clone, Copy)]
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
            'h' => symbol!(Initial, 'ㄖ'),
            'j' => symbol!(Medial, 'ㄨ'),
            'k' => symbol!(Final, 'ㄜ'),
            'l' => symbol!(Final, 'ㄠ'),
            ';' => symbol!(Final, 'ㄤ'),

            'z' => symbol!(Initial, 'ㄈ'),
            'x' => symbol!(Initial, 'ㄌ'),
            'c' => symbol!(Initial, 'ㄏ'),
            'v' => symbol!(Initial, 'ㄒ'),
            'b' => symbol!(Initial, 'ㄘ'),
            'n' => symbol!(Initial, 'ㄙ'),
            'm' => symbol!(Medial, 'ㄩ'),
            ',' => symbol!(Final, 'ㄝ'),
            '.' => symbol!(Final, 'ㄡ'),
            '/' => symbol!(Final, 'ㄥ'),

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
}

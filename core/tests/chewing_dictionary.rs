//! 驗證隨附的 `data/chewing-characters.txt`（轉換自 libchewing-data，見
//! `scripts/convert_chewing_dictionary.py` 開頭註解）可以正常載入、且與
//! 引擎其餘部分（鍵盤佈局、音節狀態機、多字詞組字）串接得起來。

use zuyin_core::{Dictionary, Engine, KeyOutcome};

fn load_chewing_dictionary() -> Dictionary {
    Dictionary::load_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/chewing-characters.txt"
    ))
    .expect("data/chewing-characters.txt 應該存在且格式正確")
}

#[test]
fn loads_a_comprehensive_number_of_characters_and_phrases() {
    let dict = load_chewing_dictionary();
    assert!(
        dict.len() > 150_000,
        "只載入了 {} 筆，詞庫可能沒放對地方或格式跑掉了",
        dict.len()
    );
}

#[test]
fn typing_a_single_character_word_then_selecting_it_finds_the_right_word() {
    let mut engine = Engine::new(load_chewing_dictionary());

    // 你 = ㄋㄧˇ : s(ㄋ) u(ㄧ) 3(ˇ)
    engine.key_press('s');
    engine.key_press('u');
    let outcome = engine.key_press('3');
    let KeyOutcome::Composing { candidates, .. } = outcome else {
        panic!("expected Composing outcome");
    };
    assert_eq!(
        candidates[0].word, "你",
        "「你」應該是 ㄋㄧˇ 最常用的候選字"
    );
    engine.select_candidate("你");
    assert_eq!(engine.buffer(), "", "選字後應清空組字區");
}

#[test]
fn typing_ni_hao_together_finds_the_real_phrase_entry() {
    // 你好是 tsi.csv 收錄的真實詞條，驗證多字詞組字（貪婪最長匹配）真的
    // 接上了隨附的正式詞庫，不是只在小型手工詞庫上測試過。
    let mut engine = Engine::new(load_chewing_dictionary());

    // 你 = ㄋㄧˇ : s(ㄋ) u(ㄧ) 3(ˇ)
    engine.key_press('s');
    engine.key_press('u');
    engine.key_press('3');

    // 好 = ㄏㄠˇ : c(ㄏ) l(ㄠ) 3(ˇ)
    engine.key_press('c');
    engine.key_press('l');
    let outcome = engine.key_press('3');
    let KeyOutcome::Composing {
        buffer, candidates, ..
    } = outcome
    else {
        panic!("expected Composing outcome");
    };
    assert_eq!(buffer, "ㄋㄧˇㄏㄠˇ");
    assert_eq!(
        candidates[0].word, "你好",
        "連續打兩個音節應該優先湊成詞庫裡的「你好」"
    );
}

#[test]
fn every_dictionary_key_round_trips_through_the_standard_keyboard_layout() {
    // 詞庫裡每一個注音字串，理論上都該是由 keyboard.rs 的按鍵組合出來的
    // 合法音節（多字詞以空白分隔每個音節，見 core/src/dictionary.rs 模組
    // 說明）；反過來說，如果詞庫混進了非法字元，這裡就是最後一道防線。
    use zuyin_core::keyboard::StandardLayout;

    let content = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/chewing-characters.txt"
    ))
    .unwrap();
    let layout = StandardLayout;

    // 反查表：注音符號 -> 是否存在對應按鍵。
    let known_glyphs: std::collections::HashSet<char> = "1234567890-qwertyuiopasdfghjkl;zxcvbnm,./"
        .chars()
        .filter_map(|k| layout.lookup(k))
        .map(|s| s.glyph)
        .collect();

    for line in content.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let zhuyin = line.split('\t').next().unwrap();
        for syllable in zhuyin.split(' ') {
            for glyph in syllable.chars() {
                assert!(
                    known_glyphs.contains(&glyph),
                    "詞庫裡的注音符號 '{glyph}'（出現在 \"{zhuyin}\"）不是大千式鍵盤認得的符號"
                );
            }
        }
    }
}

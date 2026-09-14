//! 驗證隨附的 `data/chewing-characters.txt`（轉換自 libchewing-data，見
//! `scripts/convert_chewing_dictionary.py` 開頭註解）可以正常載入、且與
//! 引擎其餘部分（鍵盤佈局、音節狀態機）串接得起來。

use zuyin_core::{Dictionary, Engine};

fn load_chewing_dictionary() -> Dictionary {
    Dictionary::load_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/chewing-characters.txt"
    ))
    .expect("data/chewing-characters.txt 應該存在且格式正確")
}

#[test]
fn loads_a_comprehensive_number_of_characters() {
    let dict = load_chewing_dictionary();
    assert!(
        dict.len() > 20_000,
        "只載入了 {} 筆，詞庫可能沒放對地方或格式跑掉了",
        dict.len()
    );
}

#[test]
fn typing_common_words_through_the_full_engine_finds_them() {
    let mut engine = Engine::new(load_chewing_dictionary());

    // 你 = ㄋㄧˇ : s(ㄋ) u(ㄧ) 3(ˇ)
    engine.key_press('s');
    engine.key_press('u');
    let outcome = engine.key_press('3');
    let zuyin_core::KeyOutcome::Composing { candidates, .. } = outcome else {
        panic!("expected Composing outcome");
    };
    assert_eq!(
        candidates[0].word, "你",
        "「你」應該是 ㄋㄧˇ 最常用的候選字"
    );

    // 好 = ㄏㄠˇ : c(ㄏ) l(ㄠ) 3(ˇ)
    engine.key_press('c');
    engine.key_press('l');
    let outcome = engine.key_press('3');
    let zuyin_core::KeyOutcome::Composing { candidates, .. } = outcome else {
        panic!("expected Composing outcome");
    };
    assert_eq!(
        candidates[0].word, "好",
        "「好」應該是 ㄏㄠˇ 最常用的候選字"
    );
}

#[test]
fn every_dictionary_key_round_trips_through_the_standard_keyboard_layout() {
    // 詞庫裡每一個注音字串，理論上都該是由 keyboard.rs 的按鍵組合出來的
    // 合法音節；反過來說，如果詞庫混進了非法字元，這裡就是最後一道防線。
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
        for glyph in zhuyin.chars() {
            assert!(
                known_glyphs.contains(&glyph),
                "詞庫裡的注音符號 '{glyph}'（出現在 \"{zhuyin}\"）不是大千式鍵盤認得的符號"
            );
        }
    }
}

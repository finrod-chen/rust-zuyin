//! 後端服務：透過 stdin/stdout 與 PIMELauncher 溝通（見
//! `docs/PIME_PROTOCOL.md`），接收按鍵事件、呼叫 zuyin-core 處理，回傳
//! 組字區內容與候選字清單。
//!
//! PIMELauncher 與 backend 子進程之間本來就是走 stdin/stdout（named pipe
//! 只存在於 client DLL 與 PIMELauncher 之間），所以不需要任何 Windows
//! 專屬的傳輸層即可完整測試這支程式。

mod protocol;
mod session;

use protocol::{
    ButtonState, CustomizeUi, MenuItem, ParsedRequest, PreservedKeyState, Reply, Request,
    ShowMessage,
};
use serde_json::json;
use session::{
    ButtonKind, ButtonSnapshot, KeyDownOutcome, MenuEntry, MessageSnapshot, Session, UiUpdate,
};
use std::collections::HashMap;
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use zuyin_core::{Dictionary, UserPhrases};

/// 使用者自訂詞庫的預設路徑（見 `zuyin_core::user_phrases` 模組文件）。
/// 刻意放在 repo 根目錄、不放進 `data/`：裡面可能是使用者自己的地址、
/// 姓名、電話等個人資料，不該被打包進版本控制（見 `.gitignore`）。
const DEFAULT_USER_PHRASES_PATH: &str = "user_phrases.txt";

fn main() -> io::Result<()> {
    let mut args = env::args().skip(1);
    let dict_path = args
        .next()
        .unwrap_or_else(|| "data/chewing-characters.txt".to_string());
    let user_phrases_path = args
        .next()
        .unwrap_or_else(|| DEFAULT_USER_PHRASES_PATH.to_string());

    let dictionary = Dictionary::load_file(&dict_path).unwrap_or_else(|err| {
        eprintln!("警告：無法載入詞庫 {dict_path}（{err}），將以空詞庫啟動");
        Dictionary::new()
    });
    let user_phrases = UserPhrases::load_file(&user_phrases_path).unwrap_or_else(|err| {
        eprintln!("警告：無法載入使用者自訂詞 {user_phrases_path}（{err}），將以空自訂詞庫啟動");
        UserPhrases::new()
    });
    eprintln!(
        "zuyin-backend 已啟動，詞庫載入 {} 筆候選字，使用者自訂詞 {} 筆",
        dictionary.len(),
        user_phrases.len()
    );

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    run(
        BufReader::new(stdin.lock()),
        &mut stdout,
        &dictionary,
        &user_phrases,
    )
}

/// 逐行讀取 `"<client_id>|<json>"` 請求、寫出 `"PIME_MSG|<client_id>|<json>"`
/// 回應。任何一行處理失敗都不能讓迴圈中斷（見 `docs/PIME_PROTOCOL.md`
/// 錯誤處理原則），因此每一步都盡量把失敗轉成回應而非提前回傳 `Err`。
fn run(
    input: impl BufRead,
    output: &mut impl Write,
    dictionary: &Dictionary,
    user_phrases: &UserPhrases,
) -> io::Result<()> {
    let mut sessions: HashMap<String, Session> = HashMap::new();
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let Some((client_id, json)) = protocol::parse_line(&line) else {
            eprintln!("ERROR: malformed request: {line}");
            continue;
        };

        match protocol::parse_request(json) {
            Ok(parsed) if matches!(parsed.request, Request::Close) => {
                sessions.remove(client_id);
                eprintln!("client disconnected: {client_id}");
            }
            Ok(parsed) => {
                let reply = dispatch(&mut sessions, client_id, dictionary, user_phrases, parsed);
                output.write_all(protocol::format_response(client_id, &reply).as_bytes())?;
                output.flush()?;
            }
            Err(err) => {
                eprintln!("ERROR: {err}: {json}");
                output.write_all(protocol::failure_response(client_id).as_bytes())?;
                output.flush()?;
            }
        }
    }
    Ok(())
}

/// 依 client 是否已完成 `init` 分派請求，對應官方 `Client.handleRequest`：
/// 尚未 `init` 前只有 `init` 有意義，其餘一律 `success: false`。
fn dispatch(
    sessions: &mut HashMap<String, Session>,
    client_id: &str,
    dictionary: &Dictionary,
    user_phrases: &UserPhrases,
    parsed: ParsedRequest,
) -> Reply {
    let ParsedRequest { seq_num, request } = parsed;
    match sessions.get_mut(client_id) {
        Some(session) => handle_initialized(session, seq_num, request),
        None => match request {
            Request::Init => {
                let mut session = Session::new(dictionary.clone());
                session.set_user_phrases(user_phrases.clone());
                sessions.insert(client_id.to_string(), session);
                Reply {
                    success: true,
                    seq_num,
                    ..Default::default()
                }
            }
            _ => Reply {
                success: false,
                seq_num,
                ..Default::default()
            },
        },
    }
}

/// 對應官方 `TextService.handleRequest` 的分派表（見 `docs/PIME_PROTOCOL.md`）。
fn handle_initialized(session: &mut Session, seq_num: u64, request: Request) -> Reply {
    match request {
        Request::OnActivate { is_keyboard_open } => {
            session.on_activate(is_keyboard_open);
            // 每次啟用都重新註冊語言列按鈕、保留鍵與候選字視窗外觀，
            // 讓 PIMELauncher 顯示目前狀態。
            let buttons = session.language_bar_buttons().map(button_state).to_vec();
            let preserved_keys = session::preserved_keys().map(preserved_key_state).to_vec();
            Reply {
                success: true,
                seq_num,
                add_button: Some(buttons),
                add_preserved_key: Some(preserved_keys),
                customize_ui: Some(customize_ui(session::candidate_ui())),
                ..Default::default()
            }
        }
        Request::OnDeactivate => {
            session.on_deactivate();
            Reply {
                success: true,
                seq_num,
                ..Default::default()
            }
        }
        Request::OnCompositionTerminated => {
            session.on_composition_terminated();
            Reply {
                success: true,
                seq_num,
                ..Default::default()
            }
        }
        Request::OnKeyboardStatusChanged { opened } => {
            let update = session.on_keyboard_status_changed(opened);
            reply_from_ui_update(seq_num, None, update)
        }
        Request::OnCommand {
            command_id,
            command_type,
        } => {
            const COMMAND_LEFT_CLICK: i64 = 0;
            if command_type != COMMAND_LEFT_CLICK {
                return Reply {
                    success: true,
                    seq_num,
                    ..Default::default()
                };
            }
            let update = session.on_command(command_id);
            reply_from_ui_update(seq_num, None, update)
        }
        Request::OnMenu { button_id } => {
            let menu = session
                .on_menu(&button_id)
                .map(|entries| entries.into_iter().map(menu_item).collect());
            Reply {
                success: true,
                seq_num,
                r#return: menu.map(|items: Vec<MenuItem>| json!(items)),
                ..Default::default()
            }
        }
        Request::OnPreservedKey { guid } => {
            let (handled, update) = session.on_preserved_key(&guid);
            reply_from_ui_update(seq_num, Some(handled), update)
        }
        Request::FilterKeyDown(event) => {
            let consumed = session.filter_key_down(&event);
            Reply {
                success: true,
                seq_num,
                r#return: Some(json!(consumed)),
                ..Default::default()
            }
        }
        Request::OnKeyDown(event) => reply_from_key_down(seq_num, session.on_key_down(&event)),
        // 官方預設行為：放開按鍵一律不處理（見 docs/PIME_PROTOCOL.md）。
        Request::FilterKeyUp | Request::OnKeyUp => Reply {
            success: true,
            seq_num,
            r#return: Some(json!(false)),
            ..Default::default()
        },
        Request::Init | Request::Close | Request::Unsupported => Reply {
            success: false,
            seq_num,
            ..Default::default()
        },
    }
}

/// 把 [`UiUpdate`]（語言列按鈕變化 + 提示訊息）組成一則回應。
/// `consumed` 是 `onPreservedKey` 專用的 `return` 布林值；其餘呼叫端
/// （`onKeyboardStatusChanged`／`onCommand`）沒有 `return` 欄位，傳
/// `None`。
fn reply_from_ui_update(seq_num: u64, consumed: Option<bool>, update: UiUpdate) -> Reply {
    Reply {
        success: true,
        seq_num,
        r#return: consumed.map(|c| json!(c)),
        change_button: update.button.map(|b| vec![button_state(b)]),
        show_message: update.message.map(show_message),
        ..Default::default()
    }
}

fn customize_ui(snapshot: session::CandidateUiSnapshot) -> CustomizeUi {
    CustomizeUi {
        cand_font_name: snapshot.font_name.to_string(),
        cand_font_size: snapshot.font_size,
        cand_per_row: snapshot.candidates_per_row,
        cand_use_cursor: snapshot.use_cursor,
    }
}

fn show_message(snapshot: MessageSnapshot) -> ShowMessage {
    ShowMessage {
        message: snapshot.text.to_string(),
        duration: snapshot.duration_secs,
    }
}

fn button_state(snapshot: ButtonSnapshot) -> ButtonState {
    let (r#type, command_id, toggled) = match snapshot.kind {
        ButtonKind::Toggle {
            command_id,
            toggled,
        } => ("toggle", Some(command_id), Some(toggled)),
        ButtonKind::Menu => ("menu", None, None),
    };
    ButtonState {
        id: snapshot.id.to_string(),
        text: snapshot.text.to_string(),
        tooltip: snapshot.tooltip.to_string(),
        r#type,
        command_id,
        toggled,
    }
}

fn menu_item(entry: MenuEntry) -> MenuItem {
    match entry {
        MenuEntry::Separator => MenuItem::separator(),
        MenuEntry::Item { text, command_id } => MenuItem::item(text, command_id),
        MenuEntry::CheckableItem {
            text,
            command_id,
            checked,
        } => MenuItem::checkable(text, command_id, checked),
    }
}

fn preserved_key_state(snapshot: session::PreservedKeySnapshot) -> PreservedKeyState {
    PreservedKeyState {
        key_code: snapshot.key_code,
        modifiers: snapshot.modifiers,
        guid: snapshot.guid.to_string(),
    }
}

fn reply_from_key_down(seq_num: u64, outcome: KeyDownOutcome) -> Reply {
    match outcome {
        KeyDownOutcome::PassThrough => Reply {
            success: true,
            seq_num,
            r#return: Some(json!(false)),
            ..Default::default()
        },
        KeyDownOutcome::Cleared => Reply {
            success: true,
            seq_num,
            r#return: Some(json!(true)),
            composition_string: Some(String::new()),
            candidate_list: Some(Vec::new()),
            show_candidates: Some(false),
            ..Default::default()
        },
        KeyDownOutcome::Composing {
            flushed,
            buffer,
            candidates,
            show_candidates,
        } => Reply {
            success: true,
            seq_num,
            r#return: Some(json!(true)),
            // 貪婪最長匹配自動收斂送出的文字（通常是空字串）跟這次的組字
            // 區內容可以同時出現在同一則回應裡：commitString 是「已經送出
            // 的既定文字」，compositionString／candidateList 是「接下來
            // 還在組的部分」，兩者互不衝突。
            commit_string: if flushed.is_empty() {
                None
            } else {
                Some(flushed)
            },
            composition_string: Some(buffer),
            candidate_list: Some(candidates),
            show_candidates: Some(show_candidates),
            ..Default::default()
        },
        KeyDownOutcome::Committed(word) => Reply {
            success: true,
            seq_num,
            r#return: Some(json!(true)),
            commit_string: Some(word),
            composition_string: Some(String::new()),
            candidate_list: Some(Vec::new()),
            show_candidates: Some(false),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dictionary() -> Dictionary {
        Dictionary::parse("ㄋㄧˇ\t你\t9000\nㄏㄠˇ\t好\t9000\nㄏㄠˇ\t號\t100\n")
    }

    fn run_lines(dictionary: &Dictionary, lines: &[&str]) -> Vec<String> {
        let input = lines.join("\n");
        let mut output = Vec::new();
        run(
            input.as_bytes(),
            &mut output,
            dictionary,
            &UserPhrases::new(),
        )
        .unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn key_event(char_code: u32, key_code: u32) -> String {
        format!(
            r#""charCode":{char_code},"keyCode":{key_code},"repeatCount":1,"scanCode":0,"isExtended":false,"keyStates":[]"#
        )
    }

    #[test]
    fn requests_before_init_fail_without_crashing() {
        let responses = run_lines(
            &sample_dictionary(),
            &[r#"c1|{"method":"onActivate","seqNum":1,"isKeyboardOpen":true}"#],
        );
        assert_eq!(
            responses,
            vec![r#"PIME_MSG|c1|{"success":false,"seqNum":1}"#]
        );
    }

    #[test]
    fn malformed_framing_is_logged_and_ignored() {
        let responses = run_lines(&sample_dictionary(), &["no-pipe-separator-here"]);
        assert!(responses.is_empty());
    }

    #[test]
    fn malformed_json_replies_with_bare_failure() {
        let responses = run_lines(&sample_dictionary(), &["c1|not json"]);
        assert_eq!(responses, vec![r#"PIME_MSG|c1|{"success":false}"#]);
    }

    #[test]
    fn full_session_lifecycle_composes_and_commits_a_word() {
        let dict = sample_dictionary();
        let responses = run_lines(
            &dict,
            &[
                r#"c1|{"method":"init","seqNum":0,"id":"guid-1","isWindows8Above":true,"isMetroApp":false,"isUiLess":false,"isConsole":false}"#,
                r#"c1|{"method":"onActivate","seqNum":1,"isKeyboardOpen":true}"#,
                &format!(
                    r#"c1|{{"method":"filterKeyDown","seqNum":2,{}}}"#,
                    key_event('s' as u32, 0x53)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":3,{}}}"#,
                    key_event('s' as u32, 0x53)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":4,{}}}"#,
                    key_event('u' as u32, 0x55)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":5,{}}}"#,
                    key_event('3' as u32, 0x33)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":6,{}}}"#,
                    key_event(' ' as u32, 0x20)
                ),
                r#"c1|{"method":"close"}"#,
            ],
        );

        assert_eq!(responses[0], r#"PIME_MSG|c1|{"success":true,"seqNum":0}"#);
        // onActivate 註冊語言列按鈕（中／英、全／半），見 addButton。
        assert!(
            responses[1].starts_with(r#"PIME_MSG|c1|{"success":true,"seqNum":1,"addButton":"#),
            "got: {}",
            responses[1]
        );
        assert_eq!(
            responses[2],
            r#"PIME_MSG|c1|{"success":true,"seqNum":2,"return":true}"#
        ); // filterKeyDown: 會吃下這個鍵

        // 最後一則按鍵回應（空白鍵確認選字）應帶有 commitString。
        let committed = responses.last().unwrap();
        assert!(
            committed.contains(r#""commitString":"你""#),
            "got: {committed}"
        );
        assert!(committed.contains(r#""seqNum":6"#));

        // "close" 不應產生任何回應。
        assert_eq!(responses.len(), 7, "close 之外的每個請求各回應一次");
    }

    #[test]
    fn two_clients_are_independent_sessions() {
        let dict = sample_dictionary();
        let responses = run_lines(
            &dict,
            &[
                r#"c1|{"method":"init","seqNum":0,"id":"guid-1"}"#,
                r#"c2|{"method":"init","seqNum":0,"id":"guid-2"}"#,
                r#"c1|{"method":"onActivate","seqNum":1,"isKeyboardOpen":true}"#,
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":2,{}}}"#,
                    key_event('s' as u32, 0x53)
                ),
                // c2 從未 onActivate，按鍵應被視為未啟用、直接 pass-through。
                &format!(
                    r#"c2|{{"method":"onKeyDown","seqNum":2,{}}}"#,
                    key_event('s' as u32, 0x53)
                ),
            ],
        );
        assert!(
            responses[3].contains(r#""compositionString":"ㄋ""#),
            "c1 應正常組字: {}",
            responses[3]
        );
        assert_eq!(
            responses[4], r#"PIME_MSG|c2|{"success":true,"seqNum":2,"return":false}"#,
            "c2 未啟用應 pass-through"
        );
    }

    #[test]
    fn typing_a_known_phrase_then_a_syllable_that_breaks_it_flushes_via_commit_string() {
        // 你好 = ㄋㄧˇ ㄏㄠˇ；是 = ㄕˋ（不接在「你好」後面）。
        let dict = Dictionary::parse(
            "ㄋㄧˇ\t你\t9000\nㄏㄠˇ\t好\t9000\nㄋㄧˇ ㄏㄠˇ\t你好\t1227\nㄕˋ\t是\t9500\n",
        );
        let responses = run_lines(
            &dict,
            &[
                r#"c1|{"method":"init","seqNum":0,"id":"guid-1"}"#,
                r#"c1|{"method":"onActivate","seqNum":1,"isKeyboardOpen":true}"#,
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":2,{}}}"#,
                    key_event('s' as u32, 0x53)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":3,{}}}"#,
                    key_event('u' as u32, 0x55)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":4,{}}}"#,
                    key_event('3' as u32, 0x33)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":5,{}}}"#,
                    key_event('c' as u32, 0x43)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":6,{}}}"#,
                    key_event('l' as u32, 0x4C)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":7,{}}}"#,
                    key_event('3' as u32, 0x33)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":8,{}}}"#,
                    key_event('g' as u32, 0x47)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":9,{}}}"#,
                    key_event('4' as u32, 0x34)
                ),
                &format!(
                    r#"c1|{{"method":"onKeyDown","seqNum":10,{}}}"#,
                    key_event('d' as u32, 0x44)
                ),
            ],
        );

        // 打完「你」「好」兩個音節後，候選字應該是詞庫裡的「你好」。
        let after_hao = &responses[7];
        assert!(
            after_hao.contains(r#""compositionString":"ㄋㄧˇㄏㄠˇ""#),
            "got: {after_hao}"
        );
        assert!(
            after_hao.contains(r#""candidateList":["你好"]"#),
            "got: {after_hao}"
        );

        // 再打「是」的聲母＋聲調，還沒破壞「你好」，尚未送出任何文字。
        let after_shi = &responses[9];
        assert!(!after_shi.contains("commitString"), "got: {after_shi}");

        // 打下一個音節的聲母（d），逼引擎發現「你好」＋「是」湊不出詞，
        // 應該把 commitString 設成自動送出的「你好」，同時繼續顯示「是」
        // 的組字區內容。
        let after_break = responses.last().unwrap();
        assert!(
            after_break.contains(r#""commitString":"你好""#),
            "got: {after_break}"
        );
        assert!(
            after_break.contains(r#""compositionString":"ㄕˋㄎ""#),
            "got: {after_break}"
        );
    }

    #[test]
    fn user_phrases_are_reachable_through_the_wire_protocol() {
        // 使用者自訂詞（見 zuyin_core::user_phrases 模組文件）透過
        // Session::set_user_phrases 掛上後，打對應注音碼應該能在候選字
        // 清單看到，且排在詞庫候選字最前面。
        let dict = Dictionary::parse("ㄈ\t分\t1000\n");
        let mut user_phrases = UserPhrases::new();
        user_phrases
            .add("ㄈ", "台北市大安區羅斯福路四段1號")
            .unwrap();

        let input = [
            r#"c1|{"method":"init","seqNum":0,"id":"guid-1"}"#.to_string(),
            r#"c1|{"method":"onActivate","seqNum":1,"isKeyboardOpen":true}"#.to_string(),
            format!(
                r#"c1|{{"method":"onKeyDown","seqNum":2,{}}}"#,
                key_event('z' as u32, 0x5A)
            ),
        ]
        .join("\n");
        let mut output = Vec::new();
        run(input.as_bytes(), &mut output, &dict, &user_phrases).unwrap();
        let responses: Vec<String> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();

        let after_key = responses.last().unwrap();
        assert!(
            after_key.contains(r#""candidateList":["台北市大安區羅斯福路四段1號","分"]"#),
            "使用者自訂詞應排在詞庫候選字最前面: {after_key}"
        );
    }
}

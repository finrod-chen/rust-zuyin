//! 後端服務：透過 stdin/stdout 與 PIMELauncher 溝通（見
//! `docs/PIME_PROTOCOL.md`），接收按鍵事件、呼叫 zuyin-core 處理，回傳
//! 組字區內容與候選字清單。
//!
//! PIMELauncher 與 backend 子進程之間本來就是走 stdin/stdout（named pipe
//! 只存在於 client DLL 與 PIMELauncher 之間），所以不需要任何 Windows
//! 專屬的傳輸層即可完整測試這支程式。

mod protocol;
mod session;

use protocol::{ButtonState, MenuItem, ParsedRequest, PreservedKeyState, Reply, Request};
use serde_json::json;
use session::{ButtonKind, ButtonSnapshot, KeyDownOutcome, MenuEntry, Session};
use std::collections::HashMap;
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use zuyin_core::Dictionary;

fn main() -> io::Result<()> {
    let dict_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "data/dict.txt".to_string());
    let dictionary = Dictionary::load_file(&dict_path).unwrap_or_else(|err| {
        eprintln!("警告：無法載入詞庫 {dict_path}（{err}），將以空詞庫啟動");
        Dictionary::new()
    });
    eprintln!(
        "zuyin-backend 已啟動，詞庫載入 {} 筆候選字",
        dictionary.len()
    );

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    run(BufReader::new(stdin.lock()), &mut stdout, &dictionary)
}

/// 逐行讀取 `"<client_id>|<json>"` 請求、寫出 `"PIME_MSG|<client_id>|<json>"`
/// 回應。任何一行處理失敗都不能讓迴圈中斷（見 `docs/PIME_PROTOCOL.md`
/// 錯誤處理原則），因此每一步都盡量把失敗轉成回應而非提前回傳 `Err`。
fn run(input: impl BufRead, output: &mut impl Write, dictionary: &Dictionary) -> io::Result<()> {
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
                let reply = dispatch(&mut sessions, client_id, dictionary, parsed);
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
    parsed: ParsedRequest,
) -> Reply {
    let ParsedRequest { seq_num, request } = parsed;
    match sessions.get_mut(client_id) {
        Some(session) => handle_initialized(session, seq_num, request),
        None => match request {
            Request::Init => {
                sessions.insert(client_id.to_string(), Session::new(dictionary.clone()));
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
            // 每次啟用都重新註冊語言列按鈕與保留鍵，讓 PIMELauncher 顯示目前狀態。
            let buttons = session.language_bar_buttons().map(button_state).to_vec();
            let preserved_keys = session::preserved_keys().map(preserved_key_state).to_vec();
            Reply {
                success: true,
                seq_num,
                add_button: Some(buttons),
                add_preserved_key: Some(preserved_keys),
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
            session.on_keyboard_status_changed(opened);
            let button = button_state(session.language_bar_buttons()[0]);
            Reply {
                success: true,
                seq_num,
                change_button: Some(vec![button]),
                ..Default::default()
            }
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
            let change_button = session
                .on_command(command_id)
                .map(|updated| vec![button_state(updated)]);
            Reply {
                success: true,
                seq_num,
                change_button,
                ..Default::default()
            }
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
            let (handled, updated) = session.on_preserved_key(&guid);
            Reply {
                success: true,
                seq_num,
                r#return: Some(json!(handled)),
                change_button: updated.map(|b| vec![button_state(b)]),
                ..Default::default()
            }
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
            buffer,
            candidates,
            show_candidates,
        } => Reply {
            success: true,
            seq_num,
            r#return: Some(json!(true)),
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
        run(input.as_bytes(), &mut output, dictionary).unwrap();
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
}

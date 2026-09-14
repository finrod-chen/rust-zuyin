//! 後端服務：Phase 2 規劃中會透過 named pipe 與 PIME 溝通（JSON 協定），
//! 接收按鍵事件、呼叫 zuyin-core 處理，並回傳組字區內容與候選字清單。
//!
//! 依 `docs/PROJECT_PLAN.md`「近期實際上手步驟」第 2 步，named pipe 傳輸
//! 尚未串接前，先以 line-delimited JSON 跑在 stdin/stdout 上，驗證協定與
//! core engine 的串接；之後只需替換傳輸層即可接上真正的 named pipe。

mod protocol;

use protocol::{Request, Response};
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use zuyin_core::{Dictionary, Engine, KeyOutcome};

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

    let mut engine = Engine::new(dictionary);
    let stdin = io::stdin();
    let stdout = io::stdout();
    run(
        &mut engine,
        BufReader::new(stdin.lock()),
        &mut stdout.lock(),
    )
}

/// 逐行讀取 JSON 請求、呼叫 engine、寫出 JSON 回應。與實際傳輸方式（stdin/stdout
/// 或未來的 named pipe）無關，方便測試。
fn run(engine: &mut Engine, input: impl BufRead, output: &mut impl Write) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handle_request(engine, request),
            Err(err) => Response {
                buffer: engine.buffer(),
                candidates: Vec::new(),
                committed: None,
                error: Some(format!("無法解析請求：{err}")),
            },
        };
        serde_json::to_writer(&mut *output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

fn handle_request(engine: &mut Engine, request: Request) -> Response {
    match request {
        Request::Key { key } => match engine.key_press(key) {
            KeyOutcome::NotHandled => Response {
                buffer: engine.buffer(),
                candidates: Vec::new(),
                committed: None,
                error: Some(format!("按鍵 '{key}' 不屬於注音鍵盤")),
            },
            KeyOutcome::Composing { buffer, candidates } => Response {
                buffer,
                candidates: candidates.into_iter().map(|e| e.word).collect(),
                committed: None,
                error: None,
            },
        },
        Request::Backspace => match engine.backspace() {
            KeyOutcome::NotHandled => unreachable!("backspace 一定會回傳 Composing"),
            KeyOutcome::Composing { buffer, candidates } => Response {
                buffer,
                candidates: candidates.into_iter().map(|e| e.word).collect(),
                committed: None,
                error: None,
            },
        },
        Request::Clear => {
            engine.clear();
            Response {
                buffer: String::new(),
                candidates: Vec::new(),
                committed: None,
                error: None,
            }
        }
        Request::Select { word } => {
            let committed = engine.select_candidate(&word);
            Response {
                buffer: String::new(),
                candidates: Vec::new(),
                committed: Some(committed),
                error: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_lines(dict: &str, requests: &[&str]) -> Vec<Response> {
        let mut engine = Engine::new(Dictionary::parse(dict));
        let input = requests.join("\n");
        let mut output = Vec::new();
        run(&mut engine, input.as_bytes(), &mut output).unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn key_events_produce_candidates() {
        let responses = run_lines(
            "ㄋㄧˇ\t你\t5000\n",
            &[
                r#"{"type":"key","key":"s"}"#,
                r#"{"type":"key","key":"u"}"#,
                r#"{"type":"key","key":"3"}"#,
            ],
        );
        assert_eq!(responses.len(), 3);
        let last = &responses[2];
        assert_eq!(last.buffer, "ㄋㄧˇ");
        assert_eq!(last.candidates, vec!["你"]);
    }

    #[test]
    fn select_commits_word_and_clears_buffer() {
        let responses = run_lines(
            "ㄋㄧˇ\t你\t5000\n",
            &[
                r#"{"type":"key","key":"s"}"#,
                r#"{"type":"key","key":"u"}"#,
                r#"{"type":"key","key":"3"}"#,
                r#"{"type":"select","word":"你"}"#,
            ],
        );
        let last = responses.last().unwrap();
        assert_eq!(last.committed.as_deref(), Some("你"));
        assert_eq!(last.buffer, "");
    }

    #[test]
    fn malformed_json_reports_error_without_crashing() {
        let responses = run_lines("", &["not json"]);
        assert_eq!(responses.len(), 1);
        assert!(responses[0].error.is_some());
    }
}

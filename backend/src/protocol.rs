//! PIME 溝通協定的 JSON 訊息格式。
//!
//! Phase 2 會透過 named pipe 傳輸這些訊息（見 `docs/PROJECT_PLAN.md`）；
//! 目前先以 line-delimited JSON 在 stdin/stdout 上運作，方便在不依賴
//! Windows 環境的情況下開發與測試。

use serde::{Deserialize, Serialize};

/// 前端（PIME）送入後端的訊息。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// 使用者按下一個鍵。
    Key { key: char },
    /// 刪除最後輸入的符號。
    Backspace,
    /// 清空目前組字狀態。
    Clear,
    /// 使用者確認選擇某個候選字。
    Select { word: String },
}

/// 後端回傳給前端的訊息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    /// 目前組字區的注音字串。
    pub buffer: String,
    /// 依目前音節查詢並排序後的候選字清單。
    pub candidates: Vec<String>,
    /// 使用者確認選字後，應送入應用程式的文字；未確認時為 `None`。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub committed: Option<String>,
    /// 請求無法處理時的錯誤說明（例如非注音鍵、JSON 格式錯誤）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_request() {
        let req: Request = serde_json::from_str(r#"{"type":"key","key":"s"}"#).unwrap();
        assert_eq!(req, Request::Key { key: 's' });
    }

    #[test]
    fn parses_select_request() {
        let req: Request = serde_json::from_str(r#"{"type":"select","word":"你"}"#).unwrap();
        assert_eq!(req, Request::Select { word: "你".into() });
    }

    #[test]
    fn response_omits_optional_fields_when_none() {
        let resp = Response {
            buffer: "ㄋㄧˇ".into(),
            candidates: vec!["你".into()],
            committed: None,
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"buffer":"ㄋㄧˇ","candidates":["你"]}"#);
    }
}

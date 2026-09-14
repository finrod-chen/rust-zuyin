//! PIME backend 線路協定：與 PIMELauncher 之間的 stdin/stdout 訊息格式。
//!
//! 詳見 `docs/PIME_PROTOCOL.md`（依官方 EasyIME/PIME Python 範例後端整理）。
//! 一行一個 UTF-8 JSON 訊息，以 `client_id` 前綴多工：
//!
//! - 請求（stdin）：`"<client_id>|<json>\n"`
//! - 回應（stdout）：`"PIME_MSG|<client_id>|<json>\n"`

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 解析一行輸入為 `(client_id, 原始 JSON 值)`；不合法的框架（缺少 `|`
/// 分隔符）沒有對應的 client，依官方行為不回應、只記錄錯誤。
pub fn parse_line(line: &str) -> Option<(&str, &str)> {
    line.split_once('|')
}

/// Windows `GetKeyboardState()` 對應的鍵盤狀態欄位，用來判斷修飾鍵是否
/// 按住（`state[code] & 0x80 != 0` 表示該鍵目前按下）。
const VK_SHIFT: usize = 0x10;
const VK_CONTROL: usize = 0x11;
const VK_MENU: usize = 0x12; // Alt

/// 按鍵事件，對應官方 `KeyEvent`（`python/textService.py`）。
///
/// 官方訊息另外還帶有 `repeatCount`／`scanCode`／`isExtended`，但目前的
/// 按鍵分類邏輯用不到，serde 反序列化時會直接忽略 JSON 裡多餘的欄位，
/// 故不在此保留（完整欄位列表見 `docs/PIME_PROTOCOL.md`）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyEventData {
    pub char_code: u32,
    pub key_code: u32,
    #[serde(default)]
    pub key_states: Vec<u8>,
}

impl KeyEventData {
    fn is_key_down(&self, code: usize) -> bool {
        self.key_states.get(code).is_some_and(|&s| s & 0x80 != 0)
    }

    /// 是否正按住 Ctrl 或 Alt。
    pub fn has_ctrl_or_alt(&self) -> bool {
        self.is_key_down(VK_CONTROL) || self.is_key_down(VK_MENU)
    }

    /// 是否正按住 Shift。用來判斷使用者是否要「跳過注音、直接輸入英文」
    /// （例如 Shift+字母），因為 charCode 已經是轉換過大小寫的結果，
    /// 單看 charCode 無法分辨。
    pub fn has_shift(&self) -> bool {
        self.is_key_down(VK_SHIFT)
    }

    /// `charCode` 若為可印出的 ASCII 字元則回傳該字元，否則回傳 `None`
    /// （例如方向鍵、功能鍵的 charCode 為 0）。
    pub fn printable_char(&self) -> Option<char> {
        char::from_u32(self.char_code).filter(|c| c.is_ascii_graphic())
    }
}

/// 已解析、依 `method` 分派後的請求。對應官方 `Client.handleRequest` /
/// `TextService.handleRequest` 的 if/elif 分派表。
#[derive(Debug, Clone)]
pub enum Request {
    /// 官方請求另帶有 `id`（TSF client GUID），但我們的 session 已經以
    /// envelope 的 `client_id` 唯一識別，不需要重複記錄，故不解析該欄位。
    Init,
    OnActivate {
        is_keyboard_open: bool,
    },
    OnDeactivate,
    FilterKeyDown(KeyEventData),
    OnKeyDown(KeyEventData),
    /// 對應官方預設行為：放開按鍵一律不處理，因此不需要保留按鍵內容。
    FilterKeyUp,
    OnKeyUp,
    OnCompositionTerminated,
    /// 語言列按鈕（或系統輸入法切換熱鍵）觸發的中／英開關狀態改變。
    OnKeyboardStatusChanged {
        opened: bool,
    },
    /// 使用者點擊有 `commandId` 的語言列按鈕，或從 `onMenu` 選單挑了一個
    /// 項目。`command_id` 是我們用 `addButton`／選單項目的 `id` 註冊的
    /// 整數識別碼——**不是**按鈕本身的字串 `id`（見 `docs/PIME_PROTOCOL.md`
    /// 「按鈕 id 與 commandId 的差異」）。
    OnCommand {
        command_id: i64,
        command_type: i64,
    },
    /// 使用者點擊 `type: "menu"` 的語言列按鈕。`button_id` 是該按鈕本身
    /// 的字串 `id`（用來決定要回傳哪個選單）。
    OnMenu {
        button_id: String,
    },
    /// 使用者按下透過 `addPreservedKey` 註冊的全域保留鍵組合。
    OnPreservedKey {
        guid: String,
    },
    /// 目前尚未實作行為的合法 method，或完全未知的 method；一律回
    /// `success: false`，與官方 `TextService.handleRequest` 的
    /// `else: success = False` 行為一致。
    Unsupported,
    /// PIMELauncher 通知 client 已斷線，伺服器端應移除該 session、不回應。
    Close,
}

/// 解析後的請求，帶著 envelope 共通的 `seqNum`。
#[derive(Debug, Clone)]
pub struct ParsedRequest {
    pub seq_num: u64,
    pub request: Request,
}

/// 解析請求 JSON。對缺少必要欄位的已知 method，容錯為 [`Request::Unsupported`]
/// 而不是回傳 `Err`——只有整個 JSON 語法都無法解析時才視為錯誤，這與官方
/// 後端「任何一行都不能讓進程掛掉」的錯誤處理原則一致。
pub fn parse_request(json: &str) -> serde_json::Result<ParsedRequest> {
    let value: Value = serde_json::from_str(json)?;
    let seq_num = value.get("seqNum").and_then(Value::as_u64).unwrap_or(0);
    let method = value.get("method").and_then(Value::as_str).unwrap_or("");

    let request = match method {
        "init" => Request::Init,
        "onActivate" => Request::OnActivate {
            is_keyboard_open: value
                .get("isKeyboardOpen")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        },
        "onDeactivate" => Request::OnDeactivate,
        "filterKeyDown" => key_event(&value).map_or(Request::Unsupported, Request::FilterKeyDown),
        "onKeyDown" => key_event(&value).map_or(Request::Unsupported, Request::OnKeyDown),
        "filterKeyUp" => Request::FilterKeyUp,
        "onKeyUp" => Request::OnKeyUp,
        "onCompositionTerminated" => Request::OnCompositionTerminated,
        "onKeyboardStatusChanged" => Request::OnKeyboardStatusChanged {
            opened: value.get("opened").and_then(Value::as_bool).unwrap_or(true),
        },
        "onCommand" => match value.get("id").and_then(Value::as_i64) {
            Some(command_id) => Request::OnCommand {
                command_id,
                command_type: value.get("type").and_then(Value::as_i64).unwrap_or(0),
            },
            None => Request::Unsupported,
        },
        "onMenu" => match value.get("id").and_then(Value::as_str) {
            Some(button_id) => Request::OnMenu {
                button_id: button_id.to_string(),
            },
            None => Request::Unsupported,
        },
        "onPreservedKey" => match value.get("guid").and_then(Value::as_str) {
            Some(guid) => Request::OnPreservedKey {
                guid: guid.to_lowercase(),
            },
            None => Request::Unsupported,
        },
        "close" => Request::Close,
        _ => Request::Unsupported,
    };
    Ok(ParsedRequest { seq_num, request })
}

fn key_event(value: &Value) -> Option<KeyEventData> {
    serde_json::from_value(value.clone()).ok()
}

/// 語言列按鈕狀態，對應官方 `addButton`／`changeButton` 累積出的 dict。
///
/// 按鈕本身的 `id`（字串）與點擊後 `onCommand` 收到的識別碼是兩回事：
/// 一般按鈕另外帶 `commandId`（整數），點擊時 `onCommand` 的 `id` 欄位
/// 送回的是這個 `commandId`，不是按鈕的字串 `id`；`type: "menu"` 的按鈕則
/// 不需要 `commandId`——點擊時觸發 `onMenu`，用按鈕自己的字串 `id` 決定
/// 要顯示哪個選單（見 `docs/PIME_PROTOCOL.md`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonState {
    pub id: String,
    pub text: String,
    pub tooltip: String,
    pub r#type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggled: Option<bool>,
}

/// 語言列選單的一個項目，對應官方 `onMenu` 回傳的 JSON 結構：
/// `{"text":..,"id":..}` 為一般項目、`{}` 為分隔線、
/// `{"text":..,"submenu":[...]}` 為子選單，皆可選配 `"checked"`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MenuItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submenu: Option<Vec<MenuItem>>,
}

impl MenuItem {
    pub fn separator() -> Self {
        Self::default()
    }

    pub fn item(text: impl Into<String>, id: i64) -> Self {
        Self {
            text: Some(text.into()),
            id: Some(id),
            ..Self::default()
        }
    }

    pub fn checkable(text: impl Into<String>, id: i64, checked: bool) -> Self {
        Self {
            text: Some(text.into()),
            id: Some(id),
            checked: Some(checked),
            ..Self::default()
        }
    }
}

/// 保留鍵註冊資訊，對應官方 `addPreservedKey`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreservedKeyState {
    pub key_code: u32,
    pub modifiers: u32,
    pub guid: String,
}

/// 候選字視窗外觀設定，對應官方 `customizeUI`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomizeUi {
    pub cand_font_name: String,
    pub cand_font_size: u32,
    pub cand_per_row: u32,
    pub cand_use_cursor: bool,
}

/// 暫時提示訊息，對應官方 `showMessage`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShowMessage {
    pub message: String,
    pub duration: u32,
}

/// 回應 JSON，對應官方 `TextService.currentReply` 累積出的欄位子集。
/// 只實作 core engine 目前用得到的欄位（見 `docs/PIME_PROTOCOL.md`
/// 「本專案 Phase 2 的取捨」）。
///
/// `return` 依 method 不同可能是 bool（`filterKeyDown`／`onKeyDown`／
/// `onPreservedKey`）或選單陣列（`onMenu`），故用 [`Value`] 而非固定型別。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub success: bool,
    pub seq_num: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#return: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composition_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_list: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_candidates: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_button: Option<Vec<ButtonState>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_button: Option<Vec<ButtonState>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_preserved_key: Option<Vec<PreservedKeyState>>,
    // 官方欄位是 "customizeUI"（UI 兩字母都大寫），rename_all = "camelCase"
    // 只會把它轉成 "customizeUi"，故需要明確覆寫。
    #[serde(rename = "customizeUI", skip_serializing_if = "Option::is_none")]
    pub customize_ui: Option<CustomizeUi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_message: Option<ShowMessage>,
}

/// 序列化一則回應為 `"PIME_MSG|<client_id>|<json>\n"`（含結尾換行）。
pub fn format_response(client_id: &str, reply: &Reply) -> String {
    format!(
        "PIME_MSG|{client_id}|{}\n",
        serde_json::to_string(reply).expect("Reply 序列化不會失敗")
    )
}

/// 通訊失敗、又無法辨識具體回應內容時的保底回應（例如 JSON 解析失敗），
/// 對應官方 `'{"success":false}'`。
pub fn failure_response(client_id: &str) -> String {
    format!("PIME_MSG|{client_id}|{{\"success\":false}}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_line_framing() {
        assert_eq!(
            parse_line(r#"abc123|{"method":"init"}"#),
            Some(("abc123", r#"{"method":"init"}"#))
        );
        assert_eq!(parse_line("no-separator"), None);
    }

    #[test]
    fn parses_init_request() {
        let parsed =
            parse_request(r#"{"method":"init","seqNum":1,"id":"guid-1","isWindows8Above":true}"#)
                .unwrap();
        assert_eq!(parsed.seq_num, 1);
        assert!(matches!(parsed.request, Request::Init));
    }

    #[test]
    fn parses_key_event_fields() {
        let parsed = parse_request(
            r#"{"method":"onKeyDown","seqNum":2,"charCode":115,"keyCode":83,
                "repeatCount":1,"scanCode":31,"isExtended":false,"keyStates":[0,0]}"#,
        )
        .unwrap();
        let Request::OnKeyDown(event) = parsed.request else {
            panic!("expected OnKeyDown");
        };
        assert_eq!(event.char_code, 115);
        assert_eq!(event.key_code, 83);
        assert_eq!(event.printable_char(), Some('s'));
    }

    #[test]
    fn unknown_method_is_unsupported_not_an_error() {
        let parsed =
            parse_request(r#"{"method":"onCompartmentChanged","seqNum":3,"guid":"x"}"#).unwrap();
        assert!(matches!(parsed.request, Request::Unsupported));
    }

    #[test]
    fn parses_on_activate_keyboard_open_flag() {
        let parsed =
            parse_request(r#"{"method":"onActivate","seqNum":1,"isKeyboardOpen":false}"#).unwrap();
        assert!(matches!(
            parsed.request,
            Request::OnActivate {
                is_keyboard_open: false
            }
        ));
    }

    #[test]
    fn parses_on_keyboard_status_changed() {
        let parsed =
            parse_request(r#"{"method":"onKeyboardStatusChanged","seqNum":1,"opened":false}"#)
                .unwrap();
        assert!(matches!(
            parsed.request,
            Request::OnKeyboardStatusChanged { opened: false }
        ));
    }

    #[test]
    fn parses_on_command() {
        let parsed = parse_request(r#"{"method":"onCommand","seqNum":1,"id":2,"type":0}"#).unwrap();
        assert!(matches!(
            parsed.request,
            Request::OnCommand {
                command_id: 2,
                command_type: 0
            }
        ));
    }

    #[test]
    fn on_command_without_id_is_unsupported() {
        let parsed = parse_request(r#"{"method":"onCommand","seqNum":1,"type":0}"#).unwrap();
        assert!(matches!(parsed.request, Request::Unsupported));
    }

    #[test]
    fn parses_on_menu() {
        let parsed =
            parse_request(r#"{"method":"onMenu","seqNum":1,"id":"zuyin-settings"}"#).unwrap();
        assert!(matches!(
            parsed.request,
            Request::OnMenu { button_id } if button_id == "zuyin-settings"
        ));
    }

    #[test]
    fn parses_on_preserved_key_and_lowercases_guid() {
        let parsed =
            parse_request(r#"{"method":"onPreservedKey","seqNum":1,"guid":"{ABCD-1234}"}"#)
                .unwrap();
        assert!(matches!(
            parsed.request,
            Request::OnPreservedKey { guid } if guid == "{abcd-1234}"
        ));
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        assert!(parse_request("not json").is_err());
    }

    #[test]
    fn ctrl_held_is_detected_from_key_states() {
        let mut key_states = vec![0u8; 32];
        key_states[VK_CONTROL] = 0x80;
        let event = KeyEventData {
            char_code: 0,
            key_code: 0x11,
            key_states,
        };
        assert!(event.has_ctrl_or_alt());
    }

    #[test]
    fn shift_held_is_detected_from_key_states() {
        let mut key_states = vec![0u8; 32];
        key_states[VK_SHIFT] = 0x80;
        let event = KeyEventData {
            char_code: 'S' as u32,
            key_code: 0x53,
            key_states,
        };
        assert!(event.has_shift());
    }

    #[test]
    fn reply_omits_unset_optional_fields() {
        let reply = Reply {
            success: true,
            seq_num: 5,
            ..Default::default()
        };
        let json = format_response("client-1", &reply);
        assert_eq!(json, "PIME_MSG|client-1|{\"success\":true,\"seqNum\":5}\n");
    }

    /// `#[serde(rename_all = "camelCase")]` 只會把 `customize_ui` 轉成
    /// `customizeUi`，但官方欄位是 `customizeUI`（U、I 都大寫的縮寫）；
    /// 曾經因為忘記加 `#[serde(rename = "customizeUI")]` 而序列化錯誤，
    /// 這個測試鎖住正確的欄位名稱，避免重蹈覆轍。同時鎖住 `ButtonState`
    /// 的 `commandId`（也曾經漏掉 `rename_all` 序列化成 `command_id`）。
    #[test]
    fn wire_field_names_for_fields_that_are_not_plain_camel_case() {
        let reply = Reply {
            success: true,
            seq_num: 1,
            customize_ui: Some(CustomizeUi {
                cand_font_name: "字型".into(),
                cand_font_size: 16,
                cand_per_row: 10,
                cand_use_cursor: false,
            }),
            add_button: Some(vec![ButtonState {
                id: "btn".into(),
                text: "文字".into(),
                tooltip: "提示".into(),
                r#type: "toggle",
                command_id: Some(1),
                toggled: Some(true),
            }]),
            ..Default::default()
        };
        let json = serde_json::to_string(&reply).unwrap();
        assert!(json.contains(r#""customizeUI":"#), "got: {json}");
        assert!(!json.contains("customizeUi"), "got: {json}");
        assert!(json.contains(r#""commandId":1"#), "got: {json}");
    }
}

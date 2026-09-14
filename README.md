# rust-zuyin

以 Rust 打造效能更佳、選字更聰明的下一代注音輸入法。

完整企劃書（緣起、系統架構、分階段規劃、風險評估）見
[`docs/PROJECT_PLAN.md`](docs/PROJECT_PLAN.md)；PIME 通訊協定研究筆記見
[`docs/PIME_PROTOCOL.md`](docs/PIME_PROTOCOL.md)。

## 專案結構

```
core/           Rust library，注音轉換核心引擎，純邏輯、可獨立測試
  keyboard.rs   注音鍵盤佈局定義（目前實作大千式）
  syllable.rs   音節狀態機
  dictionary.rs 詞庫查詢
  ranking.rs    候選字排序（詞頻 + 使用者選字記憶）
backend/        Rust binary，實作 PIME backend 通訊協定，橋接 core engine
  protocol.rs   stdin/stdout 線路格式與 PIME 訊息（method／KeyEvent／回應欄位）
  session.rs    每個 TSF client 對應一個 Session：按鍵分類、組字狀態機
  main.rs       多 client 連線管理（對應官方 Server／Client）
pime-config/    PIME 設定檔預留目錄（尚未串上 PIMELauncher）
data/
  dict.txt      範例詞庫，供開發與測試使用
docs/
  PROJECT_PLAN.md   完整專案企劃書
  PIME_PROTOCOL.md  PIME 官方後端通訊協定研究筆記
```

目前進度：

- **Phase 1（核心轉換引擎）**：鍵盤佈局、音節組合驗證、詞庫查詢、基本詞頻
  排序皆已可獨立建置與測試。
- **Phase 2（PIME 整合）進行中**：`backend/` 已實作與官方 Python 範例後端
  相同的 stdin/stdout 線路協定（`<client_id>|json` 請求／
  `PIME_MSG|<client_id>|json` 回應、`init`／`onActivate`／`filterKeyDown`／
  `onKeyDown`／`onCompositionTerminated`／`onKeyboardStatusChanged`／
  `onCommand`／`onMenu`／`onPreservedKey` 等 method），並接上 core engine
  完成組字與選字。語言列有三個按鈕：中／英、全形／半形兩個切換按鈕，以及
  一個「設定」選單按鈕（`onMenu`）；全形模式下，組字區為空時打的非注音
  符號（或按 Shift 直接打英文）會轉成全形字元送出，也可用 Shift+Space
  保留鍵（`onPreservedKey`）切換。設定選單裡可清除使用者選字記憶，操作後
  以 `showMessage` 顯示暫時提示。`onActivate` 也會用 `customizeUI` 設定
  候選字視窗外觀（數字鍵選字、每列 10 個候選字）。尚未實際安裝
  PIMELauncher 驗證（需要 Windows 環境）。

## 開發

```bash
# 建置整個 workspace
cargo build --workspace

# 執行所有單元測試
cargo test --workspace

# 手動試跑 backend（以範例詞庫，透過 stdin 逐行送入 PIME 協定訊息）
cargo run -p zuyin-backend -- data/dict.txt
```

`zuyin-backend` 的輸入／輸出協定範例（見 `docs/PIME_PROTOCOL.md` 完整說明）：

```text
# stdin（每行一則請求，格式為 "<client_id>|<json>"）
c1|{"method":"init","seqNum":0,"id":"guid-1","isWindows8Above":true,"isMetroApp":false,"isUiLess":false,"isConsole":false}
c1|{"method":"onActivate","seqNum":1,"isKeyboardOpen":true}
c1|{"method":"onKeyDown","seqNum":2,"charCode":115,"keyCode":83,"keyStates":[]}
c1|{"method":"onKeyDown","seqNum":3,"charCode":117,"keyCode":85,"keyStates":[]}
c1|{"method":"onKeyDown","seqNum":4,"charCode":51,"keyCode":51,"keyStates":[]}
c1|{"method":"onKeyDown","seqNum":5,"charCode":32,"keyCode":32,"keyStates":[]}

# stdout（格式為 "PIME_MSG|<client_id>|<json>"）
PIME_MSG|c1|{"success":true,"seqNum":0}
PIME_MSG|c1|{"success":true,"seqNum":1,"addButton":[...],"addPreservedKey":[...],"customizeUI":{"candFontName":"微軟正黑體","candFontSize":16,"candPerRow":10,"candUseCursor":false}}
PIME_MSG|c1|{"success":true,"seqNum":2,"return":true,"compositionString":"ㄋ","candidateList":[],"showCandidates":false}
PIME_MSG|c1|{"success":true,"seqNum":3,"return":true,"compositionString":"ㄋㄧ","candidateList":[],"showCandidates":false}
PIME_MSG|c1|{"success":true,"seqNum":4,"return":true,"compositionString":"ㄋㄧˇ","candidateList":["你"],"showCandidates":true}
PIME_MSG|c1|{"success":true,"seqNum":5,"return":true,"compositionString":"","commitString":"你","candidateList":[],"showCandidates":false}
```

點擊語言列「全／半」按鈕（`commandId` 為 2，見 `docs/PIME_PROTOCOL.md`
「按鈕 `id` 與 `commandId` 的差異」）、切換到全形後打的符號會直接以全形
送出：

```text
c1|{"method":"onCommand","seqNum":6,"id":2,"type":0}
c1|{"method":"onKeyDown","seqNum":7,"charCode":33,"keyCode":49,"keyStates":[]}

PIME_MSG|c1|{"success":true,"seqNum":6,"changeButton":[{"id":"zuyin-fullwidth","text":"全","tooltip":"切換全形／半形標點與符號","type":"toggle","commandId":2,"toggled":true}]}
PIME_MSG|c1|{"success":true,"seqNum":7,"return":true,"compositionString":"","commitString":"！","candidateList":[],"showCandidates":false}
```

點擊「設定」按鈕會觸發 `onMenu`；按下 Shift+Space 保留鍵也能切換全形／
半形，不需要先點按鈕：

```text
c1|{"method":"onMenu","seqNum":8,"id":"zuyin-settings"}
c1|{"method":"onPreservedKey","seqNum":9,"guid":"{9DBF7B72-A1F5-4E00-9E7A-3B1B7A2C0F01}"}

PIME_MSG|c1|{"success":true,"seqNum":8,"return":[{"text":"全形／半形輸入 (&F)","id":2,"checked":true},{},{"text":"清除使用者選字記憶 (&C)","id":3}]}
PIME_MSG|c1|{"success":true,"seqNum":9,"return":true,"changeButton":[{"id":"zuyin-fullwidth","text":"半","tooltip":"切換全形／半形標點與符號","type":"toggle","commandId":2,"toggled":false}]}
```

從設定選單選「清除使用者選字記憶」（`commandId` 為 3）沒有語言列圖示可
更新，改用 `showMessage` 顯示結果：

```text
c1|{"method":"onCommand","seqNum":10,"id":3,"type":0}

PIME_MSG|c1|{"success":true,"seqNum":10,"showMessage":{"message":"已清除使用者選字記憶","duration":2}}
```

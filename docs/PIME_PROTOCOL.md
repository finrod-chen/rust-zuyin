# PIME Backend 通訊協定筆記

依「近期實際上手步驟」第 1 步，在動手寫 Rust 之前，先研究 PIME 官方
Python 範例後端，搞清楚通訊協定長什麼樣。以下內容整理自
[EasyIME/PIME](https://github.com/EasyIME/PIME)（`HACKING.txt`、
`python/server.py`、`python/textService.py`、`python/keycodes.py`，
commit 對應 `master` 分支），僅作協定參考，本專案不使用其程式碼。

## 傳輸層：三層架構

```
PIMETextService.dll (C++, TSF client)
        │  Windows named pipe
        ▼
PIMELauncher.exe（管理各 backend 進程）
        │  子進程 stdin/stdout
        ▼
backend（Python／Node.js／本專案的 Rust 版）
```

**named pipe 只存在於 client DLL 與 PIMELauncher 之間**；PIMELauncher 與
實際的 backend（我們要寫的 Rust 程式）之間是透過子進程的 **stdin/stdout**
溝通，並非 named pipe。這代表 `backend/` 現有以 stdin/stdout 運作的設計
方向是對的，只需要把自訂的 JSON 格式換成 PIME 實際使用的格式即可，
不需要處理 Windows named pipe API。

## 訊息格式（stdin/stdout）

一行一個訊息，皆為 UTF-8 JSON，並以 `client_id` 前綴多工（一個 backend
進程可同時服務多個 TSF client session）：

- 請求（stdin）：`"<client_id>|<json>\n"`
- 回應（stdout）：`"PIME_MSG|<client_id>|<json>\n"`

（引用自 `python/server.py` 註解：）

```python
# parse PIME requests (one request per line):
# request format: "<client_id>|<JSON string>\n"
# response format: "PIME_MSG|<client_id>|<JSON string>\n"
```

特殊 method `"close"`：伺服器收到後直接移除該 client，不回應任何訊息。

錯誤處理原則（來自 `server.py` 的 `try/except` 包住整個處理迴圈）：**任何
一行請求處理失敗都不能讓 backend 進程掛掉**，否則會讓使用中的 TSF
session 卡死；失敗時回覆 `{"success": false}` 並繼續處理下一行。

## 請求／回應信封（envelope）

請求 JSON 至少帶有：

- `method`：字串，決定要呼叫的處理函式
- `seqNum`：整數，回應要原樣帶回

尚未收到 `init` 之前（`Client.service is None`），只有 `method: "init"`
有意義；其他 method 一律回 `{"seqNum":..., "success": false}`。

回應 JSON 一律帶有 `success`（bool）與 `seqNum`；若處理函式有回傳值則帶
`return`。

## `init`

由 `Client.init()` 處理（尚未進入 `TextService`），欄位：

```json
{"method": "init", "seqNum": 0, "id": "<client guid>",
 "isWindows8Above": true, "isMetroApp": false,
 "isUiLess": false, "isConsole": false}
```

成功建立 `TextService` 後回 `{"success": true, "seqNum": 0}`。

## `TextService.handleRequest` 分派的 method

依 `python/textService.py`：

| method | 請求欄位 | 說明 |
| --- | --- | --- |
| `filterKeyDown` | KeyEvent（見下） | 純查詢：這個鍵我要不要吃？不可改變狀態 |
| `onKeyDown` | KeyEvent | 實際處理按鍵，可改變組字狀態 |
| `filterKeyUp` / `onKeyUp` | KeyEvent | 同上，但對應放開按鍵；預設實作永遠回 `false`（不處理） |
| `onPreservedKey` | `guid` | 特殊保留鍵（例如切換全形／半形），本專案 Phase 2 尚未使用 |
| `onCommand` | `id`, `type` | 語言列按鈕點擊事件 |
| `onMenu` | `id` | 語言列選單 |
| `onCompartmentChanged` | `guid` | TSF compartment（如中英切換狀態）改變 |
| `onKeyboardStatusChanged` | `opened` | 輸入法開關狀態改變 |
| `onCompositionTerminated` | `forced` | 組字區被強制中止（例如切換視窗） |
| `onActivate` | `isKeyboardOpen` | 使用者切換到本輸入法 |
| `onDeactivate` | （無） | 使用者切換離開本輸入法 |

`KeyEvent` 欄位（皆為 Windows TSF／Win32 概念）：

```json
{"charCode": 115, "keyCode": 83, "repeatCount": 1,
 "scanCode": 31, "isExtended": false, "keyStates": [0, 0, ...]}
```

- `charCode`：Windows 已依 shift 狀態轉換後的 ASCII／Unicode 碼；非可顯示鍵
  （方向鍵等）為 0
- `keyCode`：Win32 虛擬鍵碼（`VK_*`，見 `python/keycodes.py`）
- `keyStates`：對應 `GetKeyboardState()`，長度 256 的鍵盤狀態陣列，用來
  判斷 Ctrl／Alt／Shift 是否按住（`state[code] & 0x80`）

## `TextService` 可在回應中設定的欄位

透過一系列 setter（`setCompositionString`／`setCandidateList`／...）寫入
`self.currentReply`，最終合併進回應 JSON：

`compositionString`、`compositionCursor`、`commitString`、`candidateList`、
`candidateCursor`、`showCandidates`、`setSelKeys`、`openKeyboard`、
`customizeUI`、`showMessage`、`hideMessage`、`addButton`、`removeButton`、
`changeButton`、`addPreservedKey`、`removePreservedKey`。

`filterKeyDown` / `onKeyDown` 等方法的回傳值（bool）會放進 `return`
欄位，代表「這個鍵是否被輸入法吃掉」。

## 本專案 Phase 2 的取捨

Rust 版 `zuyin-backend` 目前實作組字／選字，以及中／英、全／半兩個語言列
開關所需的部分：

- 訊息框架（`<client_id>|json` in / `PIME_MSG|<client_id>|json` out）
- `init` / `onActivate` / `onDeactivate` / `onCompositionTerminated`
- `onKeyboardStatusChanged`（系統輸入法切換熱鍵改變中／英狀態時）
- `onCommand`（使用者點擊語言列按鈕）
- `filterKeyDown` / `onKeyDown`（`filterKeyUp` / `onKeyUp` 維持官方預設的
  「一律不處理」行為，因為 core engine 目前不需要放開按鍵事件）
- 回應欄位用到 `compositionString`、`candidateList`、`showCandidates`、
  `commitString`、`addButton`、`changeButton`、`success`、`seqNum`、`return`

### 語言列按鈕與全形／半形

`onActivate` 回應會用 `addButton` 註冊兩個 toggle 按鈕：

- `zuyin-chinese-english`：中／英切換，對應官方 `TextService.keyboardOpen`
  （關閉時完全不攔截按鍵，所有輸入直接交還應用程式）；點擊後、或系統送
  `onKeyboardStatusChanged` 通知時，回應帶 `changeButton` 更新圖示。
- `zuyin-fullwidth`：全形／半形切換。開啟時，組字區為空、且不是任何注音
  鍵盤按鍵（或按住 Shift，使用者要跳過注音直接打英文）的可印字元，會被
  轉換成對應全形字元（Unicode `U+FF01`–`U+FF5E`，空白鍵特例轉成
  `U+3000`）後直接以 `commitString` 送出。

`onMenu`、`onPreservedKey`、`customizeUI`、`showMessage` 等其餘 UI 相關
訊息目前不需要，留待實際串上 PIMELauncher、需要對應行為時再實作。

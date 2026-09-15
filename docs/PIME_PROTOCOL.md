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

## 按鈕 `id` 與 `commandId` 的差異

研究 `addButton` 時很容易誤以為按鈕的字串 `id` 就是點擊後 `onCommand`
收到的識別碼；實際上這是兩個獨立的欄位，對照
[`chewing_ime.py`](https://github.com/EasyIME/PIME/blob/master/python/input_methods/chewing/chewing_ime.py)
（新酷音輸入法的正式 PIME 後端，非本專案程式碼，僅作協定參考）：

```python
self.addButton("switch-lang",
               icon=os.path.join(self.icon_dir, icon_name),
               tooltip="中英文切換",
               commandId=ID_SWITCH_LANG)   # ID_SWITCH_LANG = 1
```

`addButton(button_id, **kwargs)` 的實作是 `info["id"] = button_id`，
`commandId` 只是塞進 `**kwargs` 的其中一個欄位；也就是說送到 client 的
按鈕 JSON 長得像 `{"id": "switch-lang", "commandId": 1, ...}`。使用者
點擊按鈕後，client 送回的 `onCommand` 請求 `id` 欄位帶的是 `commandId`
（整數 `1`），**不是**按鈕自己的字串 `id`。

`type: "menu"` 的按鈕（例如新酷音的 `"settings"`）則不帶 `commandId`：
點擊後觸發的是 `onMenu`，`onMenu` 收到的 `id` 才是按鈕自己的字串 `id`。
選單裡個別項目的 `"id"`（見下）走的又是 `onCommand` 那組整數識別空間。

## `onMenu`：語言列選單

`type: "menu"` 的按鈕被點擊時，client 呼叫 `onMenu(buttonId)`，`return`
欄位帶一份選單結構（JSON 陣列）。同樣引用 `chewing_ime.py`：

```python
def onMenu(self, buttonId):
    if buttonId == "settings" or buttonId == "windows-mode-icon":
        return [
            {"text": "新酷音官方網站 (&W)", "id": ID_WEBSITE},
            {},  # 分隔線
            {"text": "輸出簡體中文 (&S)", "id": ID_OUTPUT_SIMP_CHINESE,
             "checked": self.outputSimpChinese},
            {"text": "網路辭典 (&D)", "submenu": [...]},  # 子選單
        ]
    return None
```

項目格式：一般項目 `{"text":..,"id":..}`；分隔線是空物件 `{}`；子選單用
`"submenu"` 帶一份巢狀陣列；可勾選項目多帶 `"checked"`。使用者選了某個
項目後，client 送回 `onCommand`，`id` 就是該項目的 `"id"`——所以選單
項目跟一般按鈕共用同一組 `commandId` 整數識別空間，`onCommand` 的處理
邏輯可以直接重用。

## `onPreservedKey`：全域保留鍵

`onActivate` 回應可用 `addPreservedKey(keyCode, modifiers, guid)` 向
client 註冊全域按鍵組合（不受目前組字狀態影響、也不需要送
`filterKeyDown`／`onKeyDown` 這組流程）。`modifiers` 是 TSF 修飾鍵旗標
（`python/textService.py` 開頭定義，見下）；`chewing_ime.py` 用這個機制
註冊 Shift+Space 做全形／半形切換：

```python
self.addPreservedKey(VK_SPACE, TF_MOD_SHIFT, SHIFT_SPACE_GUID)
```

```
TF_MOD_ALT      = 0x0001
TF_MOD_CONTROL  = 0x0002
TF_MOD_SHIFT    = 0x0004
TF_MOD_RALT     = 0x0008
TF_MOD_RCONTROL = 0x0010
TF_MOD_RSHIFT   = 0x0020
TF_MOD_LALT     = 0x0040
TF_MOD_LCONTROL = 0x0080
TF_MOD_LSHIFT   = 0x0100
```

按下註冊過的組合鍵時，client 送 `{"method":"onPreservedKey","guid":...}`
（`guid` 統一轉小寫比對），`return` 是 bool，代表是否已處理。

## `customizeUI`：候選字視窗外觀

`onActivate` 回應可用 `customizeUI(**kwargs)` 設定候選字視窗外觀，同樣
引用 `chewing_ime.py`：

```python
self.customizeUI(candFontName='MingLiu',
                 candFontSize=cfg.fontSize,
                 candPerRow=cfg.candPerRow,
                 candUseCursor=not(cfg.leftRightAction and cfg.upDownAction))
```

四個欄位：`candFontName`（字型名稱）、`candFontSize`（字級）、
`candPerRow`（每列顯示幾個候選字）、`candUseCursor`（`true` 代表用游標
／方向鍵選字、`false` 代表用數字鍵 1-9 選字）。

**踩過的坑**：`customizeUI` 這個 JSON 欄位名稱的 `UI` 兩個字母都大寫，是
不規則縮寫，不是規則的 camelCase——`Rust` 這邊若只套用
`#[serde(rename_all = "camelCase")]`，`customize_ui` 只會被轉成
`customizeUi`（小寫 i），PIMELauncher 真正期待的欄位名稱對不上。跟先前
`commandId`（見上）曾被序列化成 `command_id` 是同一類錯誤：只要欄位名稱
不是規則的 camelCase 分詞，套用 `rename_all` 後務必手動核對一次實際
輸出的 JSON，必要時加 `#[serde(rename = "...")]` 覆寫。

## `showMessage` / `hideMessage`：暫時提示訊息

`showMessage(message, duration=3)` 讓 client 顯示一個幾秒後自動消失的
提示訊息（新的呼叫會取代目前顯示的訊息）；`hideMessage()` 立刻關閉。
`chewing_ime.py` 主要用在「沒有其他畫面回饋」的操作上，例如 Ctrl+Del
刪除使用者詞彙成功／失敗的提示：

```python
self.showMessage("刪除「" + target_phrase + "」成功", 2)
```

## 安裝／註冊：`backends.json` 與 `ime.json`

以上都是 backend 進程本身的線路協定；另外還有一層「PIMELauncher 怎麼知道
要啟動哪個 backend、Windows 怎麼知道多出了一個可選的輸入法」的註冊機制，
跟線路協定完全無關，值得另外記錄（安裝腳本見
`scripts/install-windows.ps1`；設定檔本體見 `pime-config/`）。

PIMELauncher 本身其實也是用 Rust 寫的（`PIMELauncher/src/`），核心邏輯在
`backend_registry.rs`：

1. 讀 PIME 安裝根目錄下的 `backends.json`——一個陣列，每個元素是
   `{"name", "command", "workingDir", "params"}`，`command`／`workingDir`
   都是相對於 PIME 安裝根目錄的路徑。這就是「有哪些 backend 引擎」的清單。
2. 對每個 backend，掃描 `<PIME 根目錄>\<backend 名稱>\input_methods\*\`
   底下每個子資料夾的 `ime.json`，只取其中的 `"guid"` 欄位，建立
   「TSF 語言設定檔 GUID → backend 名稱」的對照表。
3. 使用者切換到某個語言設定檔時，用這個對照表找到對應的 backend，照
   `backends.json` 裡的 `command`／`workingDir`／`params` 把它當子進程
   啟動（背後就是本文件其餘部分描述的那套 stdin/stdout 協定）。

也就是說，**同一個 backend 進程可以底下掛好幾個 `input_methods\*\ime.json`
（好幾個語言設定檔／輸入法），但每個 backend 只註冊一次執行檔／工作目錄**；
`ime.json` 本身除了 `guid`（給上面第 2 步用）之外的欄位（`name`／
`locale`／`fallbackLocale`／`icon`／`win8_icon`）應該是給
Windows TSF 語言設定檔註冊、語言列顯示名稱等用途，`moduleName`／
`serviceName` 則是 Python 版 backend 才需要（動態載入對應的 Python
模組／類別）；這些欄位的實際消費者在別的元件（`PIMETextService`／
`libIME2`，C++），沒有繼續往下追。

本專案最直接可以照抄格式的先例，是官方已有的 **Go 版原生執行檔 backend**
（`go-backend/`）——跟本專案一樣是編譯成單一 `.exe`、透過 stdin/stdout
講同一套協定，不像 Python／Node.js 版需要额外的直譯器：

```json
// go-backend/README.md 引用的 backends.json 片段
{
  "name": "go-backend",
  "command": "go-backend\\server.exe",
  "workingDir": "go-backend",
  "params": ""
}
```

```json
// go-backend/input_methods/meow/ime.json（實際存在的檔案，欄位留空的
// icon／moduleName／serviceName 也照抄，代表這些欄位對原生 backend
// 可以留空）
{
	"name": "喵喵輸入法 (Go版)",
	"version": "0.1",
	"guid": "{7A1C2E93-5B64-4F88-AE21-3D9C6B70F145}",
	"locale": "zh-Hans-CN",
	"fallbackLocale": "zh-CN",
	"icon": "",
	"win8_icon": "",
	"moduleName": "",
	"serviceName": ""
}
```

官方也附了一支對應的安裝腳本
（`go-backend/deploy-server.ps1`）：停止 `PIMELauncher.exe` → 同步檔案到
PIME 安裝路徑底下 → 重新啟動 `PIMELauncher.exe`，沒有額外呼叫任何 COM／
登錄檔 API 去註冊 TSF 語言設定檔。`scripts/install-windows.ps1` 最初照這個
流程改寫（額外加上合併 `backends.json`、找 PIME 安裝路徑等步驟），假設
PIMELauncher 重啟時的動態掃描就足以讓新的語言設定檔生效。

**這個假設錯了，已經實測證實**：只做上述流程（複製檔案＋合併
`backends.json`＋重啟 `PIMELauncher.exe`）安裝後，`backends.json` 正確
更新、`PIMELauncher.exe /console` 的除錯輸出顯示運作正常，但「Rust 注音
輸入法」完全沒有出現在 Windows 的語言清單裡，PIMELauncher 也從來沒有收到
過它的 `init` 請求（只有原本就裝好的新酷音會收到）。

追下去發現：TSF 語言設定檔的註冊其實是 `PIMETextService.dll` 的
`DllRegisterServer`（`PIMETextService/DllEntry.cpp`）做的，而且只在
`regsvr32` 執行、觸發這個 DLL 的 COM 註冊進入點時才會掃描——它會走訪
`backends.json` 裡列出的每個 backend 目錄，掃描各自的
`input_methods\*\ime.json`，把每個檔案的 `guid` 都註冊成一個 TSF 語言
設定檔。`PIMELauncher.exe` 只是負責轉發 stdin/stdout 協定的常駐程式，跟
`regsvr32`／COM 註冊完全無關，重啟它不會觸發這個掃描；`installer.nsi`
在安裝當下才會呼叫

```text
regsvr32.exe /s "<PIME 安裝路徑>\x86\PIMETextService.dll"
regsvr32.exe /s "<PIME 安裝路徑>\x64\PIMETextService.dll"
```

（32／64 位元的 DLL 分開註冊，且要用對應位元的 `regsvr32.exe`——64 位元
Windows 上，32 位元版反而在 `SysWOW64` 底下，不是 `System32`，這是
Windows 由來已久的特例）。也就是說，**幫既有的 PIME 安裝新增一個
backend，事後必須重新對已經註冊過的 `PIMETextService.dll` 執行一次
`regsvr32`**，讓它重新掃描、把新加的 `ime.json` 也註冊進去，光靠重啟
`PIMELauncher.exe` 不夠。`scripts/install-windows.ps1` 已經加上這一步
（`Register-PimeTextService`），**但這個修法本身還沒有實際重新測試過**。

## 本專案 Phase 2 的取捨

Rust 版 `zuyin-backend` 目前實作組字／選字、語言列（中／英、全形／半形、
設定選單）、Shift+Space 保留鍵、候選字視窗外觀設定，以及暫時提示訊息：

- 訊息框架（`<client_id>|json` in / `PIME_MSG|<client_id>|json` out）
- `init` / `onActivate` / `onDeactivate` / `onCompositionTerminated`
- `onKeyboardStatusChanged`（系統輸入法切換熱鍵改變中／英狀態時）
- `onCommand`（點擊一般按鈕或選了選單項目）／`onMenu`（點擊選單按鈕）
- `onPreservedKey`（Shift+Space 切換全形／半形）
- `filterKeyDown` / `onKeyDown`（`filterKeyUp` / `onKeyUp` 維持官方預設的
  「一律不處理」行為，因為 core engine 目前不需要放開按鍵事件）
- 回應欄位用到 `compositionString`、`candidateList`、`showCandidates`、
  `commitString`、`addButton`、`changeButton`、`addPreservedKey`、
  `customizeUI`、`showMessage`、`success`、`seqNum`、`return`

### 語言列按鈕與全形／半形

`onActivate` 回應會用 `addButton` 註冊三個按鈕：

- `zuyin-chinese-english`（`commandId=1`，toggle）：中／英切換，對應官方
  `TextService.keyboardOpen`（關閉時完全不攔截按鍵，所有輸入直接交還
  應用程式）；點擊後、或系統送 `onKeyboardStatusChanged` 通知時，回應帶
  `changeButton` 更新圖示，後者還會附一則 `showMessage`（因為語言列圖示
  可能不在使用者視線範圍內）。
- `zuyin-fullwidth`（`commandId=2`，toggle）：全形／半形切換。開啟時，
  組字區為空、且不是任何注音鍵盤按鍵（或按住 Shift，使用者要跳過注音
  直接打英文）的可印字元，會被轉換成對應全形字元（Unicode
  `U+FF01`–`U+FF5E`，空白鍵特例轉成 `U+3000`）後直接以 `commitString`
  送出；也可用 Shift+Space 保留鍵切換（附 `showMessage` 提示切換結果，
  按鈕點擊則不附加，因為圖示變化本身已是回饋）。
- `zuyin-settings`（`type: "menu"`，無 `commandId`）：點擊觸發 `onMenu`，
  選單提供「全形／半形輸入」（可勾選、`commandId=2`，與上面的按鈕共用
  同一個處理邏輯）與「清除使用者選字記憶」（`commandId=3`，重置
  `zuyin-core::Engine::forget_selections`，讓候選字排序退回純詞頻，並以
  `showMessage` 顯示「已清除使用者選字記憶」）。

`onMenu` 之外的其餘 UI 相關訊息（`onMenu`／`onPreservedKey` 已支援；
`removeButton`／`removePreservedKey`／`setSelKeys`／`openKeyboard`／
`hideMessage` 目前不需要）留待實際串上 PIMELauncher、需要對應行為時再
實作。

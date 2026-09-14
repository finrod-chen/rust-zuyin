# pime-config

向 [PIME](https://github.com/EasyIME/PIME) 註冊本專案 `zuyin-backend` 所需的
設定檔，讓「Rust 注音輸入法」出現在 Windows 的輸入法清單裡。本專案不重新
包裝、也不重新散布 PIME 本身——這裡只是提供 PIME 認得的設定檔格式。

## 安裝

裝好 PIME 之後，執行 [`scripts/install-windows.ps1`](../scripts/install-windows.ps1)
即可自動把這裡的檔案複製到正確位置、更新 PIME 的 `backends.json`、並重啟
`PIMELauncher.exe`；細節見該腳本開頭的說明註解與 `README.md`「下載執行檔」。
以下記錄這兩個檔案各自的用途，供想手動安裝或想了解安裝腳本在做什麼的人
參考。

## `backends.json`

要合併進 `<PIME 安裝路徑>\backends.json`（一個陣列，每個 backend 一筆，
`install-windows.ps1` 會自動處理合併、不會覆蓋其他既有 backend）的片段：

```json
{
	"name": "rust-zuyin",
	"command": "rust-zuyin\\zuyin-backend.exe",
	"workingDir": "rust-zuyin",
	"params": ""
}
```

`command`／`workingDir` 都是相對於 PIME 安裝根目錄的路徑，代表
`zuyin-backend.exe` 要放在 `<PIME 安裝路徑>\rust-zuyin\` 底下、且要以那個
資料夾當作執行時的工作目錄——這樣 `zuyin-backend` 不帶參數執行時，預設的
相對路徑 `data\chewing-characters.txt` 才會剛好對到
`<PIME 安裝路徑>\rust-zuyin\data\chewing-characters.txt`。

## `input_methods/zuyin/ime.json`

要放在 `<PIME 安裝路徑>\rust-zuyin\input_methods\zuyin\ime.json`：

```json
{
	"name": "Rust 注音輸入法",
	"version": "0.1",
	"guid": "{47CC7030-55E9-4C14-B14F-8B4EAD051BA8}",
	"locale": "zh-Hant-TW",
	"fallbackLocale": "zh-TW",
	"icon": "",
	"win8_icon": "",
	"moduleName": "",
	"serviceName": ""
}
```

`guid` 是這個輸入法在 Windows TSF 裡的永久身分識別碼，**不能修改**——換了
就等於變成另一個輸入法，使用者原本的設定與語言列位置都會跟著跑掉。這個
GUID 是隨機產生、一次性寫死在這裡的，之後不會再變。

`icon`／`win8_icon` 目前留空（沒有另外設計圖示，見對照範例的
`meow`／`rime` 這兩個 Go 版輸入法也是留空的）；`moduleName`／
`serviceName` 是 Python 版 backend 才需要的欄位（用來在 Python 進程裡動態
載入對應的模組／類別），我們的 backend 不需要模組動態載入，一律留空。

## 格式依據

這兩個檔案的格式不是憑空猜的，是實際核對
[EasyIME/PIME](https://github.com/EasyIME/PIME) 這幾個檔案得出的（僅作
格式參考，本專案不使用其程式碼；細節與引用見
[`docs/PIME_PROTOCOL.md`](../docs/PIME_PROTOCOL.md)「安裝／註冊」一節）：

- `PIMELauncher/src/backend_registry.rs`：PIMELauncher 本身（也是 Rust
  寫的）怎麼讀 `backends.json`、怎麼掃描每個 backend 的
  `input_methods\*\ime.json` 取得 `guid`
- `go-backend/README.md`、`go-backend/deploy-server.ps1`：官方已有的
  「原生編譯執行檔」backend 範例（Go 版），跟本專案用 Rust 編譯出單一
  `.exe` 的情況幾乎一樣，是最直接可對照的先例
- `go-backend/input_methods/{meow,rime}/ime.json`：兩份實際存在、格式
  跟 Python 版 `ime.json` 一致的真實範例檔案

**這一步沒有在真正的 Windows／PIME 環境驗證過**（開發環境沒有 Windows
機器）——上面這些格式都是照著官方原始碼與範例檔案核對過的，但沒有實際
跑過 `PIMELauncher.exe` 確認輸入法真的會出現在 Windows 的語言清單裡。如果
照 `install-windows.ps1` 的步驟做完、輸入法還是沒有出現，請照實回報看到
的狀況（錯誤訊息、`PIMELauncher.exe /console` 的除錯輸出等），一起排查，
不要照抄其他輸入法的教學自己改設定。

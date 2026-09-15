# pime-config

向 [PIME](https://github.com/EasyIME/PIME) 註冊本專案 `zuyin-backend` 所需的
設定檔，讓「Rust 注音輸入法」出現在 Windows 的輸入法清單裡。本專案不重新
包裝、也不重新散布 PIME 本身——這裡只是提供 PIME 認得的設定檔格式。

## 安裝

裝好 PIME 之後，執行 [`scripts/install-windows.ps1`](../scripts/install-windows.ps1)
即可自動把這裡的檔案複製到正確位置、更新 PIME 的 `backends.json`、對
`PIMETextService.dll` 重新執行 `regsvr32`、並重啟 `PIMELauncher.exe`；細節
見該腳本開頭的說明註解與 `README.md`「下載執行檔」。以下記錄這兩個檔案
各自的用途，供想手動安裝或想了解安裝腳本在做什麼的人參考。

**重新執行 `regsvr32` 這一步是必要的**：第一次實測時只做了複製檔案、更新
`backends.json`、重啟 `PIMELauncher.exe`，`PIMELauncher.exe /console` 的
除錯輸出顯示一切正常，但「Rust 注音輸入法」完全沒有出現在 Windows 的
語言清單、也從來沒收到過任何 `init` 請求。原因見下面「格式依據」——TSF
語言設定檔的註冊是 `PIMETextService.dll` 的 `DllRegisterServer` 做的一次性
掃描，只有 `regsvr32` 執行時才會觸發，PIMELauncher.exe 重啟不會。已經把
這一步加進安裝腳本。

想完整移除，執行 [`scripts/uninstall-windows.ps1`](../scripts/uninstall-windows.ps1)
（見 README.md「解除安裝」）；不要手動刪檔案了事，TSF 語言設定檔的
註冊／解除註冊是整個 `PIMETextService.dll` 共用同一個 CLSID，只刪資料夾
會在 Windows 語言清單留下一個指向不存在檔案的殘影項目，細節見那支腳本
開頭的說明註解。

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
- `PIMETextService/DllEntry.cpp` 的 `DllRegisterServer`：確認 TSF 語言
  設定檔的註冊時機（`regsvr32` 執行時的一次性掃描），以及
  `installer/installer.nsi` 裡官方安裝程式怎麼呼叫
  `regsvr32.exe /s "...\PIMETextService.dll"`（32／64 位元的 DLL 分開
  註冊，用對應位元的 `regsvr32.exe`）

另外核對過 `PIMETextService/DllEntry.cpp` 的 `DllRegisterServer`，確認了
上面提到的「`regsvr32` 才會觸發 TSF 語言設定檔掃描」這件事，也是
`install-windows.ps1` 現在會重新執行 `regsvr32` 的依據。

**第一輪安裝腳本（只做複製檔案＋更新 `backends.json`＋重啟
`PIMELauncher.exe`）已經實測過，確認不夠**：`backends.json` 正確更新、
`PIMELauncher.exe /console` 顯示運作正常，但「Rust 注音輸入法」沒有出現在
Windows 的語言清單，也沒有收到任何 `init` 請求——加上 `regsvr32` 這一步
之後**還沒有重新測試過**。如果照更新後的 `install-windows.ps1` 跑完，
輸入法還是沒有出現，請照實回報看到的狀況（錯誤訊息、regsvr32 的結束碼、
`PIMELauncher.exe /console` 的除錯輸出等），一起排查，不要照抄其他輸入法
的教學自己改設定。

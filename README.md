# rust-zuyin

以 Rust 打造效能更佳、選字更聰明的下一代注音輸入法。

完整企劃書（緣起、系統架構、分階段規劃、風險評估）見
[`docs/PROJECT_PLAN.md`](docs/PROJECT_PLAN.md)；PIME 通訊協定研究筆記見
[`docs/PIME_PROTOCOL.md`](docs/PIME_PROTOCOL.md)；詞庫資料的授權見
[`docs/THIRD_PARTY_NOTICES.md`](docs/THIRD_PARTY_NOTICES.md)。

## 專案結構

```
core/           Rust library，注音轉換核心引擎，純邏輯、可獨立測試
  keyboard.rs   注音鍵盤佈局定義（大千式、倚天式；見下方「鍵盤佈局」）
  syllable.rs   單一音節的狀態機
  dictionary.rs 詞庫查詢；也維護多音節詞的「合法前綴」索引、縮寫索引、
                不分聲調索引
  ranking.rs    候選字排序（詞頻 + 使用者選字記憶）
  user_phrases.rs 使用者自訂詞（地址／姓名／電話等快速填寫捷徑）
  lib.rs        Engine：多音節組字狀態機，實作貪婪最長匹配（見下）
  tests/        對隨附詞庫檔案（data/chewing-characters.txt）的整合測試
backend/        Rust binary，實作 PIME backend 通訊協定，橋接 core engine
  protocol.rs   stdin/stdout 線路格式與 PIME 訊息（method／KeyEvent／回應欄位）
  session.rs    每個 TSF client 對應一個 Session：按鍵分類、組字狀態機
  main.rs       多 client 連線管理（對應官方 Server／Client）
pime-config/    PIME 設定檔預留目錄（尚未串上 PIMELauncher）
scripts/
  convert_chewing_dictionary.py  把 libchewing-data 轉成本專案詞庫格式
data/
  dict.txt                範例詞庫（手工撰寫，供文件範例與快速測試使用）
  chewing-characters.txt  正式詞庫：轉換自 libchewing-data 的單字讀音與
                           多字詞，約 16 萬筆、附真實詞頻，backend 預設
                           載入這份
  user_phrases.example.txt 使用者自訂詞範例格式（實際使用請複製成
                           repo 根目錄的 user_phrases.txt，見上方「使用者
                           自訂詞」說明）
docs/
  PROJECT_PLAN.md          完整專案企劃書
  PIME_PROTOCOL.md         PIME 官方後端通訊協定研究筆記
  THIRD_PARTY_NOTICES.md   詞庫資料的第三方授權聲明
```

目前進度：

- **Phase 1（核心轉換引擎）：完成**。鍵盤佈局、音節組合驗證、詞庫查詢、
  基本 bigram 詞頻排序皆已可獨立建置與測試；詞庫已改用轉換自
  libchewing-data 的正式詞庫（`data/chewing-characters.txt`，單字與多字詞
  共約 16 萬筆，附真實詞頻），不再只是十幾筆的範例資料。企劃書原本列的
  IBM／精業／許氏鍵盤佈局，因為實務上很少人用，先不實作（只做大千式與
  倚天式這兩種最常用的）。
  **鍵盤佈局**：預設大千式（Windows 內建標準），可透過
  `Engine::set_layout`／`zuyin-backend` 第三個命令列參數（`standard`／
  `eten`）切換成倚天式；兩份鍵位表都對照過
  [libchewing](https://github.com/chewing/libchewing) 的權威實作核對過
  （見 `core/src/keyboard.rs`）——過程中也發現並修正了大千式 `h`／`b`
  兩個鍵先前寫反的問題（`h` 應該是 ㄘ、`b` 應該是 ㄖ）。
  **基本 bigram 詞頻排序**：候選字排序除了詞頻與個人選字記憶，也會看
  「上一個送出的字」＋這個候選字兩字連起來是不是詞庫裡的真實詞，是的話
  用那個詞的真實語料詞頻加權，讓排序多少反映上下文——例如剛送出「我」
  以後，同音字裡跟「我」常常組成詞的字會被排到前面（見 `core/src/lib.rs`
  模組文件「基本 bigram 詞頻排序」）。直接重用詞庫本來就有的片語詞頻
  資料當訊號，沒有另外收集或訓練語言模型，是刻意做得很「基本」的版本。
  **多字詞（片語）組字**：`core::Engine` 支援連續打好幾個音節，採用
  「貪婪最長匹配」——只要目前累積的音節序列還可能湊成詞庫裡更長的詞，
  就持續累積、即時顯示最長的可能詞當候選字；一旦再打下一個音節就湊不出
  任何詞了，就自動把目前累積裡最長的完整詞送出，再從新音節重新開始（見
  `core/src/lib.rs` 模組文件）。例如連續打「ㄋㄧˇ」「ㄏㄠˇ」會直接候選
  「你好」，不是分別選兩個單字。
  **注音縮寫輸入**：仿照手機注音輸入法，只打每個字的第一個符號（聲母，
  沒聲母則是介母／韻母，不含聲調）也能預測多字詞——例如連續打兩次
  ㄒ（不接任何介母／韻母／聲調）會候選「謝謝」「熊熊」「行銷」等兩個
  音節開頭都是 ㄒ 的詞（見 `core/src/lib.rs` 模組文件「注音縮寫輸入」與
  `core/src/dictionary.rs` 的 `lookup_abbreviation`）。
  **不分聲調選字**：打完聲母／介母／韻母、還沒（或不想）打聲調時，也
  能直接選字——引擎會把同一個基底讀音、所有聲調的候選字都列出來，例如
  打 ㄊㄞ（不接聲調）會同時列出「台」「太」「胎」，不必先打對聲調才能
  選（見 `core/src/lib.rs` 模組文件「不分聲調選字」與
  `core/src/dictionary.rs` 的 `lookup_toneless`）。
  **使用者自訂詞（快速填寫）**：可以自己定義「打一組注音 → 送出一段
  任意文字」的捷徑，例如把地址、姓名、電話設成自訂詞，在瀏覽器或文件
  裡快速填寫（見 `core/src/user_phrases.rs`）。自訂詞一律排在候選字清單
  最前面。目前的使用方式是直接編輯自訂詞檔案（預設路徑
  `user_phrases.txt`，格式見 `data/user_phrases.example.txt`；這個檔案
  不會被版本控制追蹤，見 `.gitignore`），`zuyin-backend` 啟動時會自動
  載入；透過 PIME 介面直接新增／刪除自訂詞則有待未來擴充（PIME 的線路
  協定沒有通用文字輸入框，見 `docs/PIME_PROTOCOL.md`），但 `core::Engine`
  已經有 `add_user_phrase`／`remove_user_phrase` 這組程式介面可供未來的
  設定介面呼叫。
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

# 手動試跑 backend（預設載入 data/chewing-characters.txt 正式詞庫、
# user_phrases.txt 使用者自訂詞（不存在時視為空，不是錯誤），透過 stdin
# 逐行送入 PIME 協定訊息）
cargo run -p zuyin-backend

# 想用文件範例裡的小型詞庫（下面範例、單元測試用的就是這份）：
cargo run -p zuyin-backend -- data/dict.txt

# 想順便試用自訂詞範例（見 data/user_phrases.example.txt）：
cp data/user_phrases.example.txt user_phrases.txt
cargo run -p zuyin-backend -- data/chewing-characters.txt user_phrases.txt

# 想改用倚天式鍵盤佈局（第三個參數，預設 standard；見上方「鍵盤佈局」）：
cargo run -p zuyin-backend -- data/chewing-characters.txt user_phrases.txt eten

# 重新產生 data/chewing-characters.txt（來源與授權見
# docs/THIRD_PARTY_NOTICES.md）：
curl -o /tmp/word.csv https://raw.githubusercontent.com/chewing/libchewing-data/master/dict/chewing/word.csv
curl -o /tmp/tsi.csv  https://raw.githubusercontent.com/chewing/libchewing-data/master/dict/chewing/tsi.csv
python3 scripts/convert_chewing_dictionary.py /tmp/word.csv /tmp/tsi.csv > data/chewing-characters.txt
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

連續打兩個音節而不確認選字，候選字會是詞庫裡的多字詞；再打下一個音節
若接不上，引擎會自動把目前累積裡最長的完整詞透過 `commitString` 送出
（貪婪最長匹配，見上方「多字詞組字」說明），組字區則接著顯示新音節：

```text
c1|{"method":"onKeyDown","seqNum":6,"charCode":99,"keyCode":67,"keyStates":[]}
c1|{"method":"onKeyDown","seqNum":7,"charCode":108,"keyCode":76,"keyStates":[]}
c1|{"method":"onKeyDown","seqNum":8,"charCode":51,"keyCode":51,"keyStates":[]}

PIME_MSG|c1|{"success":true,"seqNum":6,"return":true,"compositionString":"ㄋㄧˇㄏ","candidateList":[],"showCandidates":false}
PIME_MSG|c1|{"success":true,"seqNum":7,"return":true,"compositionString":"ㄋㄧˇㄏㄠ","candidateList":[],"showCandidates":false}
PIME_MSG|c1|{"success":true,"seqNum":8,"return":true,"compositionString":"ㄋㄧˇㄏㄠˇ","candidateList":["你好","妳好"],"showCandidates":true}
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

只打每個字的第一個符號（不接介母／韻母／聲調）也能觸發縮寫聯想；連打
兩次 ㄒ（鍵盤上是 `v`）就會列出兩個音節開頭都是 ㄒ 的詞，例如「謝謝」
「行銷」「熊熊」：

```text
c1|{"method":"onKeyDown","seqNum":11,"charCode":118,"keyCode":86,"keyStates":[]}
c1|{"method":"onKeyDown","seqNum":12,"charCode":118,"keyCode":86,"keyStates":[]}

PIME_MSG|c1|{"success":true,"seqNum":11,"return":true,"compositionString":"ㄒ","candidateList":["ㄒ"],"showCandidates":true}
PIME_MSG|c1|{"success":true,"seqNum":12,"return":true,"compositionString":"ㄒㄒ","candidateList":["謝謝","行銷","熊熊", "..."],"showCandidates":true}
```

打完聲母／介母／韻母、不接聲調也能直接選字——候選字會列出同一個讀音
所有聲調的字，例如打 `w`（ㄊ）`9`（ㄞ）不按聲調鍵：

```text
c1|{"method":"onKeyDown","seqNum":13,"charCode":119,"keyCode":87,"keyStates":[]}
c1|{"method":"onKeyDown","seqNum":14,"charCode":57,"keyCode":57,"keyStates":[]}

PIME_MSG|c1|{"success":true,"seqNum":13,"return":true,"compositionString":"ㄊ","candidateList":[],"showCandidates":false}
PIME_MSG|c1|{"success":true,"seqNum":14,"return":true,"compositionString":"ㄊㄞ","candidateList":["台","太","抬","胎","..."],"showCandidates":true}
```

如果載入了使用者自訂詞（見上方「使用者自訂詞」說明），打對應的捷徑
注音碼會直接叫出自訂文字，且排在詞庫候選字最前面；下例載入了
`data/user_phrases.example.txt`，打一個 `ㄉ`（鍵盤上是 `2`）：

```text
c1|{"method":"onKeyDown","seqNum":15,"charCode":50,"keyCode":50,"keyStates":[]}

PIME_MSG|c1|{"success":true,"seqNum":15,"return":true,"compositionString":"ㄉ","candidateList":["台北市大安區羅斯福路四段1號"],"showCandidates":true}
```

選這個候選字就會把整段地址當作 `commitString` 送出，直接貼進瀏覽器
表單或文件裡的輸入欄位。

基本 bigram 詞頻排序會依「上一個送出的字」調整候選字順序：送出「我」
以後打 ㄇㄣˊ，「們」（跟「我」常組成「我們」）會被排到比基礎詞頻更高
的「門」前面：

```text
c1|{"method":"onKeyDown","seqNum":16,"charCode":97,"keyCode":65,"keyStates":[]}
c1|{"method":"onKeyDown","seqNum":17,"charCode":112,"keyCode":80,"keyStates":[]}
c1|{"method":"onKeyDown","seqNum":18,"charCode":54,"keyCode":54,"keyStates":[]}

PIME_MSG|c1|{"success":true,"seqNum":16,"return":true,"compositionString":"ㄇ","candidateList":["ㄇ"],"showCandidates":true}
PIME_MSG|c1|{"success":true,"seqNum":17,"return":true,"compositionString":"ㄇㄣ","candidateList":["悶","燜","們","門","...","..."],"showCandidates":true}
PIME_MSG|c1|{"success":true,"seqNum":18,"return":true,"compositionString":"ㄇㄣˊ","candidateList":["們","門","穈","捫","...","..."],"showCandidates":true}
```

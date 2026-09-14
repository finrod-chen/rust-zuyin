# rust-zuyin

以 Rust 打造效能更佳、選字更聰明的下一代注音輸入法。

完整企劃書（緣起、系統架構、分階段規劃、風險評估）見
[`docs/PROJECT_PLAN.md`](docs/PROJECT_PLAN.md)。

## 專案結構

```
core/           Rust library，注音轉換核心引擎，純邏輯、可獨立測試
  keyboard.rs   注音鍵盤佈局定義（目前實作大千式）
  syllable.rs   音節狀態機
  dictionary.rs 詞庫查詢
  ranking.rs    候選字排序（詞頻 + 使用者選字記憶）
backend/        Rust binary，橋接 core engine 與外部輸入法框架
  main.rs       目前以 line-delimited JSON 跑在 stdin/stdout 上；
                Phase 2 將改為透過 named pipe 與 PIME 溝通
pime-config/    PIME 設定檔預留目錄（Phase 2）
data/
  dict.txt      範例詞庫，供開發與測試使用
docs/
  PROJECT_PLAN.md  完整專案企劃書
```

目前進度對應企劃書 Phase 1（核心轉換引擎）：鍵盤佈局、音節組合驗證、詞庫
查詢、基本詞頻排序皆已可獨立建置與測試；Phase 2（PIME 整合）尚未開始。

## 開發

```bash
# 建置整個 workspace
cargo build --workspace

# 執行所有單元測試
cargo test --workspace

# 手動試跑 backend（以範例詞庫，透過 stdin 逐行送入 JSON 按鍵事件）
cargo run -p zuyin-backend -- data/dict.txt
```

`zuyin-backend` 的輸入／輸出協定範例：

```jsonc
// stdin（每行一則請求）
{"type":"key","key":"s"}
{"type":"key","key":"u"}
{"type":"key","key":"3"}
{"type":"select","word":"你"}

// stdout（對應每則請求的回應）
{"buffer":"ㄋ","candidates":[]}
{"buffer":"ㄋㄧ","candidates":[]}
{"buffer":"ㄋㄧˇ","candidates":["你"]}
{"buffer":"","candidates":[],"committed":"你"}
```

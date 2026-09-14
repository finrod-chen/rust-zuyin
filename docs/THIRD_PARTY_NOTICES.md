# 第三方授權聲明

本專案程式碼採用 MIT 授權（見 `Cargo.toml` 的 `license = "MIT"`）。以下
資料檔案來自第三方、採用不同授權，**不屬於** MIT 授權範圍，使用時請保留
其原始授權與出處：

## `data/chewing-characters.txt`

由 `scripts/convert_chewing_dictionary.py` 轉換自
[libchewing-data](https://github.com/chewing/libchewing-data) 的
`dict/chewing/word.csv` 與 `dict/chewing/tsi.csv`（僅取用其中的單字讀音
與詞頻，未收錄多字詞——原因見該指令碼開頭註解）。

- 授權：**LGPL-2.1-or-later**（見來源檔案開頭的 `dc:license` 註解）
- 著作權：Copyright (c) libchewing Core Team
- 取得方式與轉換方式：見 `scripts/convert_chewing_dictionary.py`

`data/chewing-characters.txt` 本身是上述 LGPL 授權資料的衍生／重新格式化
版本，散布時應一併保留本聲明與來源連結。若對「MIT 專案內含 LGPL 授權的
資料檔案」是否符合你的散布或商業使用情境有疑慮，建議另行諮詢法律意見；
本專案僅單純標明來源與授權，不構成法律建議。

## `data/dict.txt`

純手工撰寫的小型範例詞庫，供開發與測試使用，與 `data/chewing-characters.txt`
無關，屬本專案原創內容（MIT 授權）。

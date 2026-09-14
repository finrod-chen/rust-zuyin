#!/usr/bin/env python3
"""將 libchewing-data 的字庫／詞庫轉成本專案的詞庫格式
（zhuyin<TAB>word<TAB>freq）。

單字讀音與多字詞（片語）都會轉換。多字詞的 zhuyin 欄位保留來源檔案本來
就有的空白分隔（每個音節一個 token，例如「你好」是 "ㄋㄧˇ ㄏㄠˇ"）：
core::Dictionary 用這個空白位置切出音節邊界，才能在使用者連續打好幾個
音節時，判斷目前累積的音節序列是不是詞庫裡某個詞的合法前綴（見
core/src/dictionary.rs 的 `is_valid_prefix`）。單一音節的詞條本來就沒有
空白，天然相容。

資料來源（LGPL-2.1-or-later，見各檔案開頭的 dc:rights／dc:license 註解）：
    https://github.com/chewing/libchewing-data
    dict/chewing/word.csv  單字字庫：每個字的每種讀音，詞頻多半是 0
                           （libchewing 用檔案內的先後順序表示同音字的
                           預設優先順序，而非數字詞頻）
    dict/chewing/tsi.csv   內建詞庫：包含單字與多字詞，單字部分詞頻多為
                           實際語料統計值，比 word.csv 準確

重新取得原始檔案：
    curl -o word.csv https://raw.githubusercontent.com/chewing/libchewing-data/master/dict/chewing/word.csv
    curl -o tsi.csv  https://raw.githubusercontent.com/chewing/libchewing-data/master/dict/chewing/tsi.csv

用法：
    python3 scripts/convert_chewing_dictionary.py word.csv tsi.csv > data/chewing-characters.txt
"""

import sys


def load_csv(path):
    """讀取 word.csv／tsi.csv：每行 `word,freq,zhuyin`，`#` 開頭是註解。"""
    rows = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split(",")
            if len(parts) != 3:
                continue
            word, freq, zhuyin = parts
            rows.append((word, int(freq), zhuyin))
    return rows


def main():
    if len(sys.argv) != 3:
        print(f"用法: {sys.argv[0]} <word.csv> <tsi.csv>", file=sys.stderr)
        return 1

    word_rows = load_csv(sys.argv[1])
    tsi_rows = load_csv(sys.argv[2])

    # 單字讀音：tsi.csv 的詞頻比較準確，優先採用；word.csv 只用來補
    # tsi.csv 沒有收錄的極少數讀音（詞頻沿用 word.csv 原本的 0）。
    merged = {}
    for word, freq, zhuyin in word_rows:
        if len(word) == 1:
            merged[(word, zhuyin)] = freq
    for word, freq, zhuyin in tsi_rows:
        if len(word) == 1:
            merged[(word, zhuyin)] = freq

    # 多字詞：只有 tsi.csv 有，直接收錄，詞頻就是語料統計值。
    for word, freq, zhuyin in tsi_rows:
        if len(word) > 1:
            merged[(word, zhuyin)] = freq

    by_zhuyin = {}
    for (word, zhuyin), freq in merged.items():
        by_zhuyin.setdefault(zhuyin, []).append((word, freq))

    print("# 本檔案由 scripts/convert_chewing_dictionary.py 自動產生，")
    print("# 請勿手動編輯；來源與重新產生方式見該指令碼開頭註解。")
    print("# 來源：libchewing-data（LGPL-2.1-or-later）")
    print("# Copyright (c) libchewing Core Team")
    for zhuyin in sorted(by_zhuyin):
        # 同音（詞）依詞頻由高到低排序；詞頻相同時保留來源檔案的原始相對
        # 順序（word.csv／tsi.csv 本身就是依預設優先順序排列）。
        entries = sorted(by_zhuyin[zhuyin], key=lambda e: -e[1])
        for word, freq in entries:
            print(f"{zhuyin}\t{word}\t{freq}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

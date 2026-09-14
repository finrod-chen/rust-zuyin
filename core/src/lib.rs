//! Zhuyin（注音）輸入法核心引擎。
//!
//! 此 crate 為平台無關的純邏輯 library，不依賴任何特定作業系統 GUI 或
//! 輸入法框架（如 PIME／TSF），方便獨立測試與優化，也方便未來擴充到其他
//! 平台。詳見 `docs/PROJECT_PLAN.md`。
//!
//! ## 多字詞（片語）組字：貪婪最長匹配
//!
//! `Engine` 一次可以累積不只一個音節：使用者連續打完一個音節的所有符號
//! 後接著打下一個音節時（也就是 [`syllable::Syllable::push`] 因為某個
//! 分類的槽位已經填過而回傳 `Rejected`），引擎不會馬上捨棄前一個音節，
//! 而是嘗試把它併入目前正在組的「多音節序列」——只要這個序列仍是詞庫裡
//! 某個詞的合法前綴（[`dictionary::Dictionary::is_valid_prefix`]），就
//! 繼續累積、暫不送出任何文字，讓候選字清單即時反映目前整串音節能對上
//! 的詞。
//!
//! 一旦再累積下去就湊不出詞庫裡任何詞（新音節加進去後不再是合法前綴），
//! 引擎就在這個當下「收斂」：在已累積的音節序列裡，從最長的前綴開始往
//! 短找，只要某個前綴長度剛好是詞庫裡一個完整詞條，就把那個詞（依詞頻
//! ／使用者記憶排序後的第一名）當作「自動送出」的文字，這就是「貪婪最
//! 長匹配」——優先送出找得到的最長詞，而不是逐字送出。送出之後，序列
//! 裡沒被這次匹配用掉的剩餘音節（若有）會重新嘗試比對，可能連續收斂
//! 好幾次，直到剩下的音節又能繼續當作合法前綴累積、或完全用盡為止；
//! 因此一次按鍵理論上可能一口氣自動送出不只一個詞（[`KeyOutcome::Composing`]
//! 的 `flushed` 是 `Vec<String>`），只是实务上很少發生。
//!
//! 這整個過程都不會呼叫 [`ranking::Ranker::record_selection`]——自動送出
//! 的是引擎當下排序第一的猜測，不代表使用者真的比較過候選字、主動選了
//! 它，所以不該影響之後的個人化排序；只有透過 [`Engine::select_candidate`]
//! 明確選字才會被記住。
//!
//! ## 注音縮寫輸入（仿手機輸入法）
//!
//! 除了完整音節組字，引擎也支援「只打每個字的第一個符號」來預測多字詞，
//! 模仿手機注音輸入法常見的縮寫聯想（見
//! [`dictionary::Dictionary::lookup_abbreviation`] 的縮寫碼定義）。當
//! 目前累積的音節（已確認 + 正在輸入中）全部都只打了一個符號、且至少有
//! 兩個音節時（[`Engine::abbreviation_code`]），引擎會額外把這些符號串
//! 起來查詢縮寫索引，把結果併入候選字清單（見 [`Engine::refresh_candidates`]）。
//! 例如連續打兩個 ㄒ（各自只打聲母、不接任何介母／韻母／聲調），候選字
//! 就會出現「謝謝」「熊熊」「行銷」等兩個音節開頭都是 ㄒ 的詞。這與貪婪
//! 最長匹配是同一套音節累積機制，只是額外多查一次縮寫索引，不影響一般
//! 完整音節組字與自動收斂的行為。
//!
//! ## 不分聲調選字
//!
//! 使用者打完聲母／介母／韻母、但還沒（或不想）打聲調時，[`Engine::toneless_key`]
//! 會額外查詢 [`dictionary::Dictionary::lookup_toneless`]，把同一個基底
//! 讀音、所有聲調的候選字都併入候選字清單，不必先打對聲調才能選字。這
//! 只在「目前正在輸入的音節」還沒打聲調時觸發；一旦打了聲調，就只會用
//! 一般的精確比對，不會再混入其他聲調的候選字。
//!
//! 這代表候選字視窗可能在使用者打完聲調「之前」就已經開啟——平台整合層
//! 如果單純以「候選字視窗已開啟」判斷數字鍵一律是選字鍵（常見的候選字
//! UI 慣例），會不小心把接下來要打的聲調數字鍵也吃掉。
//! [`Engine::extends_current_syllable`] 就是設計來讓平台整合層在這種
//! 情況下優先判斷「這個鍵還能不能繼續組字」，見該方法文件；
//! `backend::Session::classify` 已經套用這個判斷。
//!
//! ## 使用者自訂詞（快速填寫）
//!
//! 除了詞庫本身，引擎也可以掛上一份 [`user_phrases::UserPhrases`]（見該
//! 模組文件），讓使用者自行定義「打一組注音 → 送出一段任意文字」的捷徑
//! ——例如把地址、姓名、電話設成自訂詞，在瀏覽器或文件裡快速填寫。自訂
//! 詞永遠優先於詞庫本身與縮寫／不分聲調的猜測結果（見
//! [`Engine::refresh_candidates`] 的合併順序），因為那是使用者自己明確
//! 設定的捷徑，不是引擎的猜測。
//!
//! ## 鍵盤佈局
//!
//! `Engine` 預設用大千式（[`keyboard::StandardLayout`]），可用
//! [`Engine::set_layout`] 切換成倚天式（[`keyboard::EtenLayout`]），見
//! [`keyboard::KeyboardLayout`]。
//!
//! ## 基本 bigram 詞頻排序
//!
//! 排序候選字時，除了詞庫詞頻與使用者個人選字記憶，還會額外看「上一個
//! 送到應用程式的字（`last_committed`）＋這個候選字」兩個字連起來是否
//! 剛好是詞庫裡的一個真實詞（[`dictionary::Dictionary::word_frequency`]
//! 反查），是的話用那個詞的真實語料詞頻當加權，讓候選字排序多少能反映
//! 上下文，而不是每個字獨立地只看自己的詞頻——例如剛送出「我」以後，
//! 同音字裡跟「我」常常連在一起組成詞的字會被排到前面。這是刻意做得很
//! 「基本」的版本：直接重用詞庫本來就有的片語詞頻資料當 bigram 訊號，
//! 沒有另外收集或訓練語言模型（見 `docs/PROJECT_PLAN.md` Phase 1
//! 「基本 bigram/trigram 詞頻排序」）。`last_committed` 由
//! [`Engine::select_candidate`] 與貪婪最長匹配自動送出的文字更新，
//! [`Engine::reset_context`] 可以手動清空（例如切換應用程式、輸入框失焦
//! 時，見 `backend` 的 `Session::reset_composition`）；`backspace`／
//! `clear` 不會影響它，因為那些操作是在修改「還沒送出」的內容，不代表
//! 上一個已經送出的字改變了。

pub mod dictionary;
pub mod keyboard;
pub mod ranking;
pub mod syllable;
pub mod user_phrases;

pub use dictionary::{Dictionary, Entry};
pub use keyboard::KeyboardLayout;
pub use user_phrases::UserPhrases;

use keyboard::ZhuyinSymbol;
use ranking::Ranker;
use std::io;
use syllable::{PushResult, Syllable};

/// 呼叫端送入一個按鍵後，引擎的回應。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    /// 按鍵不屬於注音鍵盤，呼叫端應自行處理（例如直接輸入該字元、或視為
    /// 一般英數字元送出）。
    NotHandled,
    /// 按鍵已接受，組字區內容如下；候選字清單依目前累積的音節序列查詢
    /// 並排序。
    Composing {
        /// 這個按鍵若觸發了貪婪最長匹配的自動收斂（見本模組文件），這裡
        /// 依序是被自動送出的文字；絕大多數情況下是空陣列。呼叫端應將
        /// 這些文字視為緊接在上一次確認輸出之後、且先於這次組字區內容
        /// 的既定輸出（例如接在一起設成同一個 `commitString`）。
        flushed: Vec<String>,
        buffer: String,
        candidates: Vec<Entry>,
    },
}

/// 注音輸入法核心引擎：組合鍵盤佈局、音節狀態機、詞庫與排序模型。
pub struct Engine {
    layout: KeyboardLayout,
    /// 已確認屬於目前多音節序列、但尚未送出的音節（見模組文件）。
    committed: Vec<Syllable>,
    /// 正在輸入中的音節。
    current: Syllable,
    dictionary: Dictionary,
    /// 使用者自訂詞（見模組文件「使用者自訂詞」），預設空。
    user_phrases: UserPhrases,
    ranker: Ranker,
    /// 上一個送到應用程式的字（見模組文件「基本 bigram 詞頻排序」）。
    last_committed: Option<String>,
}

impl Engine {
    pub fn new(dictionary: Dictionary) -> Self {
        Self {
            layout: KeyboardLayout::default(),
            committed: Vec::new(),
            current: Syllable::new(),
            dictionary,
            user_phrases: UserPhrases::new(),
            ranker: Ranker::new(),
            last_committed: None,
        }
    }

    /// 切換鍵盤佈局（見模組文件「鍵盤佈局」）。
    pub fn set_layout(&mut self, layout: KeyboardLayout) {
        self.layout = layout;
    }

    /// 清空「上一個送出的字」這個 bigram 排序用的上下文（見模組文件
    /// 「基本 bigram 詞頻排序」）。適合在應用程式切換、輸入框失焦等
    /// 「接下來打的字跟前面已經送出的字其實沒有語意關聯」的時機呼叫。
    pub fn reset_context(&mut self) {
        self.last_committed = None;
    }

    /// 掛上一份使用者自訂詞庫，取代目前這份（見模組文件「使用者自訂
    /// 詞」）。
    pub fn set_user_phrases(&mut self, user_phrases: UserPhrases) {
        self.user_phrases = user_phrases;
    }

    /// 目前掛載的使用者自訂詞庫，供列出／管理用。
    pub fn user_phrases(&self) -> &UserPhrases {
        &self.user_phrases
    }

    /// 新增一筆使用者自訂詞（見 [`UserPhrases::add`]）。
    pub fn add_user_phrase(&mut self, code: &str, text: &str) -> io::Result<()> {
        self.user_phrases.add(code, text)
    }

    /// 移除一筆使用者自訂詞（見 [`UserPhrases::remove`]）。
    pub fn remove_user_phrase(&mut self, code: &str, text: &str) -> io::Result<bool> {
        self.user_phrases.remove(code, text)
    }

    /// 目前組字區的注音字串（已確認音節 + 正在輸入的音節，依序串接、
    /// 不含分隔符號，供畫面顯示用）。
    pub fn buffer(&self) -> String {
        self.display_buffer()
    }

    /// 這個按鍵是否屬於目前的注音鍵盤佈局。供呼叫端（例如平台整合層）
    /// 在不觸發任何狀態改變的情況下，判斷是否該把按鍵交給這個引擎處理。
    pub fn supports_key(&self, key: char) -> bool {
        self.layout.lookup(key).is_some()
    }

    /// 這個按鍵送進 [`Engine::key_press`] 會不會單純疊加進「正在輸入中
    /// 的音節」（而不是觸發音節邊界，或者根本不是注音鍵）。
    ///
    /// 用於呼叫端（例如平台整合層）在候選字視窗已經開啟時，判斷某個
    /// 按鍵該優先當成「繼續組字」還是「選字」——尤其是「不分聲調選字」
    /// （見模組文件）可能讓候選字視窗在使用者打完聲調之前就已經開啟，
    /// 若這時完全依賴「候選字視窗開啟中」來判斷數字鍵一律是選字鍵，會
    /// 誤吃掉原本該接續輸入的聲調數字鍵。只要這個鍵對應的類別（聲母／
    /// 介母／韻母／聲調）在目前音節裡還是空的，就回傳 `true`，呼叫端
    /// 應該優先讓它繼續組字。
    pub fn extends_current_syllable(&self, key: char) -> bool {
        match self.layout.lookup(key) {
            Some(symbol) => !self.current.has(symbol.kind),
            None => false,
        }
    }

    /// 處理一個按鍵事件。
    pub fn key_press(&mut self, key: char) -> KeyOutcome {
        let Some(symbol) = self.layout.lookup(key) else {
            return KeyOutcome::NotHandled;
        };

        let flushed = if self.current.push(symbol) == PushResult::Rejected {
            // 同類別符號已填過：這個音節結束了，使用者要開始下一個音節。
            self.advance_to_next_syllable(symbol)
        } else {
            Vec::new()
        };
        if let Some(last) = flushed.last() {
            // 貪婪最長匹配自動送出的文字，也是真的送到應用程式的字，
            // 該當作接下來 bigram 排序的上下文（見模組文件）。
            self.last_committed = Some(last.clone());
        }

        self.refresh_candidates(flushed)
    }

    /// 刪除最後輸入的符號。若目前音節已空，會把上一個已確認音節「還原」
    /// 回輸入中狀態，再刪除它的最後一個符號——使用者的觀感是「一次刪一
    /// 個符號」，不會因為符號剛好落在音節邊界而整個音節一次消失。
    pub fn backspace(&mut self) -> KeyOutcome {
        if !self.current.is_empty() {
            self.current.backspace();
        } else if let Some(mut last) = self.committed.pop() {
            last.backspace();
            self.current = last;
        }
        self.refresh_candidates(Vec::new())
    }

    /// 清空目前組字狀態（例如使用者按 Esc）。
    pub fn clear(&mut self) {
        self.committed.clear();
        self.current.clear();
    }

    /// 清除所有已累積的使用者選字記憶，候選字排序退回純依詞庫詞頻。
    pub fn forget_selections(&mut self) {
        self.ranker.clear();
    }

    /// 使用者確認選字：記錄使用者記憶並清空組字狀態，回傳應送入應用程式的文字。
    pub fn select_candidate(&mut self, word: &str) -> String {
        let key = self.full_key();
        self.ranker.record_selection(&key, word);
        self.clear();
        self.last_committed = Some(word.to_string());
        word.to_string()
    }

    /// 音節邊界處理：把剛結束的音節併入 `committed`，必要時觸發貪婪最長
    /// 匹配收斂（見模組文件），最後把觸發邊界的這個符號放進全新的
    /// `current`，開始下一個音節。
    fn advance_to_next_syllable(&mut self, symbol: ZhuyinSymbol) -> Vec<String> {
        let finished = std::mem::take(&mut self.current);
        let mut flushed = Vec::new();

        loop {
            let mut tentative = self.committed.clone();
            tentative.push(finished.clone());
            if self.dictionary.is_valid_prefix(&Self::join_key(&tentative)) {
                // 加入這個音節後仍有機會湊成詞庫裡的詞，繼續累積、暫不送出。
                self.committed = tentative;
                break;
            }

            // 加進去就湊不出任何詞了：在目前已累積的音節裡，從最長的前綴
            // 開始找，第一個是詞庫完整詞條的前綴就是這次要送出的詞。
            if let Some((cut_len, word)) = self.longest_complete_match(&self.committed) {
                flushed.push(word);
                self.committed.drain(0..cut_len);
                // 剩下沒被這次匹配用掉的音節，重新嘗試接上 `finished`。
                continue;
            }

            if !self.committed.is_empty() {
                // 已累積的音節本身、以及它的任何前綴都不是詞庫裡的完整
                // 詞條（理論上很罕見）：沒有東西可以送出，只能捨棄，避免
                // 卡在無限迴圈。
                self.committed.clear();
                continue;
            }

            // `committed` 已經是空的、`finished` 自己也不構成任何詞的合法
            // 前綴（讀音本身就不在詞庫裡）：沒有詞可以延伸或送出，原封
            // 不動保留這個音節，讓使用者看到組字區內容、可以自行刪除。
            self.committed = vec![finished.clone()];
            break;
        }

        self.current.push(symbol); // 一定成功：current 剛清空
        flushed
    }

    /// 在 `syllables` 裡，從最長的前綴開始找第一個是詞庫完整詞條的前綴，
    /// 回傳它的音節數與（依詞頻／使用者記憶排序後）第一名候選字。
    fn longest_complete_match(&self, syllables: &[Syllable]) -> Option<(usize, String)> {
        for len in (1..=syllables.len()).rev() {
            let key = Self::join_key(&syllables[..len]);
            let entries = self.dictionary.lookup(&key);
            if let Some(best) = self.ranker.rank(&key, entries).first() {
                return Some((len, best.word.clone()));
            }
        }
        None
    }

    /// 依序查詢並合併四種候選字來源，順序即優先順序（見模組文件）：
    /// 1. 使用者自訂詞（明確設定的捷徑，永遠優先）
    /// 2. 一般詞庫精確比對
    /// 3. 注音縮寫猜測（若符合觸發條件）
    /// 4. 不分聲調猜測（若符合觸發條件）
    ///
    /// 四種來源一律用 `key`（[`Engine::full_key`]）當排序記憶鍵，因為
    /// [`Engine::select_candidate`] 一律以它記錄使用者選字記憶——不論候選
    /// 字最終是從哪個來源找到的，都要共用同一份個人化排序記憶才有意義。
    /// 每個來源內部也都會依「基本 bigram 詞頻排序」（見模組文件）做次要
    /// 排序，但不會打亂來源之間的優先順序（例如使用者自訂詞就算沒有
    /// bigram 加權，也一定排在詞庫候選字之前——這是 call 的先後順序保證
    /// 的，跟每個來源內部怎麼排無關）。
    fn refresh_candidates(&self, flushed: Vec<String>) -> KeyOutcome {
        let buffer = self.display_buffer();
        let key = self.full_key();
        let mut candidates: Vec<Entry> = Vec::new();

        if !key.is_empty() {
            self.merge_ranked(&key, self.user_phrases.lookup(&key), &mut candidates);
            self.merge_ranked(&key, self.dictionary.lookup(&key), &mut candidates);
        }
        if let Some(code) = self.abbreviation_code() {
            self.merge_ranked(
                &key,
                self.dictionary.lookup_abbreviation(&code),
                &mut candidates,
            );
        }
        if let Some(base) = self.toneless_key() {
            self.merge_ranked(
                &key,
                self.dictionary.lookup_toneless(&base),
                &mut candidates,
            );
        }

        KeyOutcome::Composing {
            flushed,
            buffer,
            candidates,
        }
    }

    /// 依 `rank_key` 排序 `entries`（詞頻＋使用者記憶），再依「跟上一個
    /// 送出的字連起來是否為真實詞」（見模組文件「基本 bigram 詞頻排序」）
    /// 做一次穩定的次要排序，最後把還沒出現過（依詞比對）的候選字依序
    /// 附加到 `candidates` 尾端。
    fn merge_ranked(&self, rank_key: &str, entries: &[Entry], candidates: &mut Vec<Entry>) {
        let mut ranked = self.ranker.rank(rank_key, entries);
        ranked.sort_by_key(|entry| std::cmp::Reverse(self.bigram_boost(&entry.word)));
        for candidate in ranked {
            if !candidates
                .iter()
                .any(|existing| existing.word == candidate.word)
            {
                candidates.push(candidate.clone());
            }
        }
    }

    /// `word` 接在「上一個送出的字」後面是否剛好是詞庫裡的真實詞，是的
    /// 話回傳那個詞的真實語料詞頻，否則回傳 0（見模組文件「基本 bigram
    /// 詞頻排序」）。
    fn bigram_boost(&self, word: &str) -> u32 {
        match &self.last_committed {
            Some(prev) => self.dictionary.word_frequency(&format!("{prev}{word}")),
            None => 0,
        }
    }

    /// 若目前累積的音節（已確認 + 正在輸入中）都只打了「第一個符號」
    /// （見 [`syllable::Syllable::leading_glyph`]），且至少有兩個音節，
    /// 回傳依序串起來的縮寫碼，供 [`dictionary::Dictionary::lookup_abbreviation`]
    /// 查詢；否則回傳 `None`（例如任一音節已經打了不只一個符號，代表
    /// 使用者是在正常輸入完整音節，不是縮寫輸入）。
    fn abbreviation_code(&self) -> Option<String> {
        let mut syllables: Vec<&Syllable> = self.committed.iter().collect();
        if self.current.is_ready() {
            syllables.push(&self.current);
        }
        if syllables.len() < 2 {
            return None;
        }

        let mut code = String::new();
        for syllable in syllables {
            let zhuyin = syllable.as_zhuyin_string();
            if zhuyin.chars().count() != 1 {
                return None;
            }
            code.push_str(&zhuyin);
        }
        Some(code)
    }

    /// 若正在輸入中的音節已經可查詢（聲母／介母／韻母至少有一個）但還
    /// 沒打聲調，回傳「已確認音節＋正在輸入中的音節，一律拿掉聲調」的
    /// 字串，供 [`dictionary::Dictionary::lookup_toneless`] 查詢；否則
    /// 回傳 `None`（例如組字區還是空的，或使用者已經打了聲調——這種情況
    /// 只該用一般精確比對，不該混入其他聲調的候選字，見模組文件「不分
    /// 聲調選字」）。
    fn toneless_key(&self) -> Option<String> {
        if !self.current.is_ready() || self.current.has_tone() {
            return None;
        }
        let mut parts: Vec<String> = self
            .committed
            .iter()
            .map(Syllable::base_zhuyin_string)
            .collect();
        parts.push(self.current.base_zhuyin_string());
        Some(parts.join(" "))
    }

    /// 供畫面顯示用的組字區內容：已確認音節與正在輸入的音節依序串接，
    /// 不含分隔符號。
    fn display_buffer(&self) -> String {
        let mut buffer: String = self
            .committed
            .iter()
            .map(Syllable::as_zhuyin_string)
            .collect();
        buffer.push_str(&self.current.as_zhuyin_string());
        buffer
    }

    /// 供詞庫查詢用的鍵：已確認音節加上（若已可查詢）正在輸入的音節，
    /// 以空白分隔（見 [`dictionary`] 模組說明）。
    fn full_key(&self) -> String {
        let mut parts: Vec<String> = self
            .committed
            .iter()
            .map(Syllable::as_zhuyin_string)
            .collect();
        if self.current.is_ready() {
            parts.push(self.current.as_zhuyin_string());
        }
        parts.join(" ")
    }

    fn join_key(syllables: &[Syllable]) -> String {
        syllables
            .iter()
            .map(Syllable::as_zhuyin_string)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dictionary() -> Dictionary {
        Dictionary::parse(
            "ㄋㄧˇ\t你\t5000\n\
             ㄏㄠˇ\t好\t5000\n\
             ㄕˋ\t是\t9000\n\
             ㄨㄛˇ\t我\t9000\n\
             ㄊㄞˊ\t台\t3000\n\
             ㄨㄢ\t灣\t2000\n",
        )
    }

    #[test]
    fn typing_ni_then_hao_yields_expected_candidates() {
        let mut engine = Engine::new(sample_dictionary());

        // 你 = ㄋㄧˇ : s(ㄋ) u(ㄧ) 3(ˇ)
        engine.key_press('s');
        engine.key_press('u');
        let outcome = engine.key_press('3');
        match outcome {
            KeyOutcome::Composing {
                flushed,
                buffer,
                candidates,
            } => {
                assert_eq!(flushed, Vec::<String>::new());
                assert_eq!(buffer, "ㄋㄧˇ");
                assert_eq!(
                    candidates,
                    vec![Entry {
                        word: "你".into(),
                        frequency: 5000
                    }]
                );
            }
            other => panic!("unexpected outcome: {other:?}"),
        }

        let committed = engine.select_candidate("你");
        assert_eq!(committed, "你");
        assert_eq!(engine.buffer(), "");

        // 好 = ㄏㄠˇ : c(ㄏ) l(ㄠ) 3(ˇ)
        engine.key_press('c');
        engine.key_press('l');
        let outcome = engine.key_press('3');
        match outcome {
            KeyOutcome::Composing {
                flushed,
                buffer,
                candidates,
            } => {
                assert_eq!(flushed, Vec::<String>::new());
                assert_eq!(buffer, "ㄏㄠˇ");
                assert_eq!(
                    candidates,
                    vec![Entry {
                        word: "好".into(),
                        frequency: 5000
                    }]
                );
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn tai_wan_round_trips_through_full_pipeline() {
        let mut engine = Engine::new(sample_dictionary());

        // 台 = ㄊㄞˊ : w(ㄊ) 9(ㄞ) 6(ˊ)
        engine.key_press('w');
        engine.key_press('9');
        let outcome = engine.key_press('6');
        assert_eq!(
            outcome,
            KeyOutcome::Composing {
                flushed: Vec::new(),
                buffer: "ㄊㄞˊ".into(),
                candidates: vec![Entry {
                    word: "台".into(),
                    frequency: 3000
                }],
            }
        );
        engine.select_candidate("台");

        // 灣 = ㄨㄢ (第一聲無聲調符號) : j(ㄨ) 0(ㄢ)
        engine.key_press('j');
        let outcome = engine.key_press('0');
        assert_eq!(
            outcome,
            KeyOutcome::Composing {
                flushed: Vec::new(),
                buffer: "ㄨㄢ".into(),
                candidates: vec![Entry {
                    word: "灣".into(),
                    frequency: 2000
                }],
            }
        );
    }

    #[test]
    fn non_zhuyin_key_is_not_handled() {
        let mut engine = Engine::new(Dictionary::new());
        assert_eq!(engine.key_press('!'), KeyOutcome::NotHandled);
    }

    #[test]
    fn supports_key_does_not_mutate_state() {
        let engine = Engine::new(Dictionary::new());
        assert!(engine.supports_key('s'));
        assert!(!engine.supports_key('!'));
        assert_eq!(engine.buffer(), "", "純查詢不應改變組字狀態");
    }

    #[test]
    fn selecting_a_candidate_boosts_it_above_base_frequency_ranking_next_time() {
        let dict = Dictionary::parse("ㄏㄠˇ\t好\t250\nㄏㄠˇ\t號\t100\n");
        let mut engine = Engine::new(dict);

        engine.key_press('c');
        engine.key_press('l');
        engine.key_press('3');
        engine.select_candidate("號"); // 使用者選了詞頻較低的「號」

        // 每次確認選字都會清空組字區，所以要重打一次音節才能再記錄一次選字。
        engine.key_press('c');
        engine.key_press('l');
        engine.key_press('3');
        engine.select_candidate("號"); // 再選一次以確保加權足以超越基礎詞頻差距

        engine.key_press('c');
        engine.key_press('l');
        let outcome = engine.key_press('3');
        match outcome {
            KeyOutcome::Composing { candidates, .. } => {
                assert_eq!(candidates[0].word, "號", "使用者記憶應提升「號」的排序");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn forget_selections_resets_ranking_to_base_frequency() {
        let dict = Dictionary::parse("ㄏㄠˇ\t好\t250\nㄏㄠˇ\t號\t100\n");
        let mut engine = Engine::new(dict);

        engine.key_press('c');
        engine.key_press('l');
        engine.key_press('3');
        engine.select_candidate("號");
        engine.key_press('c');
        engine.key_press('l');
        engine.key_press('3');
        engine.select_candidate("號");

        engine.forget_selections();

        engine.key_press('c');
        engine.key_press('l');
        let outcome = engine.key_press('3');
        match outcome {
            KeyOutcome::Composing { candidates, .. } => {
                assert_eq!(candidates[0].word, "好", "清除記憶後應退回純詞頻排序");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    /// 你好 = ㄋㄧˇ ㄏㄠˇ；打完兩個音節，候選字應該是「你好」這個詞，
    /// 而不是分別打兩個單字。
    fn phrase_dictionary() -> Dictionary {
        Dictionary::parse(
            "ㄋㄧˇ\t你\t9000\n\
             ㄏㄠˇ\t好\t9000\n\
             ㄏㄠˇ\t號\t100\n\
             ㄋㄧˇ ㄏㄠˇ\t你好\t1227\n\
             ㄕˋ\t是\t9500\n",
        )
    }

    #[test]
    fn typing_two_syllables_of_a_known_phrase_yields_the_phrase_candidate() {
        let mut engine = Engine::new(phrase_dictionary());

        // 你 = ㄋㄧˇ : s u 3
        engine.key_press('s');
        engine.key_press('u');
        engine.key_press('3');
        // 好 = ㄏㄠˇ : c l 3
        engine.key_press('c');
        engine.key_press('l');
        let outcome = engine.key_press('3');

        assert_eq!(
            outcome,
            KeyOutcome::Composing {
                flushed: Vec::new(),
                buffer: "ㄋㄧˇㄏㄠˇ".into(),
                candidates: vec![Entry {
                    word: "你好".into(),
                    frequency: 1227
                }],
            }
        );
    }

    #[test]
    fn selecting_the_phrase_records_memory_under_the_full_multi_syllable_key() {
        let mut engine = Engine::new(phrase_dictionary());
        engine.key_press('s');
        engine.key_press('u');
        engine.key_press('3');
        engine.key_press('c');
        engine.key_press('l');
        engine.key_press('3');

        let committed = engine.select_candidate("你好");
        assert_eq!(committed, "你好");
        assert_eq!(engine.buffer(), "", "選字後應完全清空多音節緩衝");
    }

    #[test]
    fn typing_a_third_syllable_that_breaks_the_phrase_flushes_it_greedily() {
        let mut engine = Engine::new(phrase_dictionary());

        // 你好 = ㄋㄧˇ ㄏㄠˇ
        engine.key_press('s');
        engine.key_press('u');
        engine.key_press('3');
        engine.key_press('c');
        engine.key_press('l');
        engine.key_press('3');

        // 是 = ㄕˋ：g(ㄕ) 4(ˋ)。到這裡為止，「你好是」還沒被判定失敗——
        // 因為還沒有下一個音節能證明「是」接不上，候選字清單就是先前
        // 測試驗證過的「你好」（見 typing_two_syllables_of_a_known_phrase_...）。
        engine.key_press('g');
        engine.key_press('4');

        // 直到再打下一個音節的聲母（d=ㄎ），才會發現「你好」＋「是」
        // 湊不出詞庫裡任何詞：貪婪最長匹配這時才會在已累積的音節裡，
        // 自動送出其中最長的完整詞條「你好」，並從「是」開始重新累積
        // （「是」本身仍是合法前綴，繼續保留在組字區，「ㄎ」則是下一個
        // 音節剛起頭的聲母）。
        let outcome = engine.key_press('d');
        assert_eq!(
            outcome,
            KeyOutcome::Composing {
                flushed: vec!["你好".to_string()],
                buffer: "ㄕˋㄎ".into(),
                candidates: Vec::new(),
            }
        );
    }

    #[test]
    fn typing_syllable_not_extending_any_word_flushes_the_previous_one_greedily() {
        // 沒有以「你」開頭的詞（本測試詞庫只有單字），打完「是」以後、
        // 再打下一個音節的聲母，應該會自動送出「你」，而不是卡住或誤觸發
        // 不存在的詞。
        let dict = Dictionary::parse("ㄋㄧˇ\t你\t9000\nㄕˋ\t是\t9500\n");
        let mut engine = Engine::new(dict);

        engine.key_press('s');
        engine.key_press('u');
        engine.key_press('3');

        // 是 = ㄕˋ : g(ㄕ) 4(ˋ)
        engine.key_press('g');
        engine.key_press('4');

        // d = ㄎ，下一個音節的聲母，逼引擎判斷「你」＋「是」湊不出詞。
        let outcome = engine.key_press('d');
        assert_eq!(
            outcome,
            KeyOutcome::Composing {
                flushed: vec!["你".to_string()],
                buffer: "ㄕˋㄎ".into(),
                candidates: Vec::new(),
            }
        );
    }

    /// 謝謝／熊熊／行銷三個詞的頭兩個音節開頭都是 ㄒ，用來驗證縮寫輸入
    /// （見模組文件「注音縮寫輸入」）。
    fn abbreviation_dictionary() -> Dictionary {
        Dictionary::parse(
            "ㄒㄧㄝˋ ㄒㄧㄝˋ\t謝謝\t500\n\
             ㄒㄩㄥˊ ㄒㄩㄥˊ\t熊熊\t100\n\
             ㄒㄧㄥˊ ㄒㄧㄠ\t行銷\t800\n\
             ㄋㄧˇ ㄏㄠˇ\t你好\t1227\n",
        )
    }

    #[test]
    fn typing_two_bare_initials_predicts_words_sharing_those_leading_glyphs() {
        let mut engine = Engine::new(abbreviation_dictionary());

        // v = ㄒ（聲母）。連打兩次 v：第一個 ㄒ 因為聲母槽位已填而觸發
        // 音節邊界，第二個 ㄒ 開始新音節；兩個音節都只打了聲母。
        engine.key_press('v');
        let outcome = engine.key_press('v');

        let KeyOutcome::Composing {
            buffer, candidates, ..
        } = outcome
        else {
            panic!("expected Composing outcome");
        };
        assert_eq!(buffer, "ㄒㄒ");
        let mut words: Vec<&str> = candidates.iter().map(|e| e.word.as_str()).collect();
        words.sort();
        assert_eq!(words, vec!["熊熊", "行銷", "謝謝"]);
    }

    #[test]
    fn abbreviation_candidates_can_be_selected_like_normal_candidates() {
        let mut engine = Engine::new(abbreviation_dictionary());
        engine.key_press('v');
        engine.key_press('v');

        let committed = engine.select_candidate("行銷");
        assert_eq!(committed, "行銷");
        assert_eq!(engine.buffer(), "", "選字後應清空組字區");
    }

    #[test]
    fn a_single_bare_initial_does_not_trigger_abbreviation_matching() {
        // 只打了一個音節（還沒有第二個）不該觸發縮寫查詢。
        let mut engine = Engine::new(abbreviation_dictionary());
        let outcome = engine.key_press('v');
        let KeyOutcome::Composing { candidates, .. } = outcome else {
            panic!("expected Composing outcome");
        };
        assert!(candidates.is_empty());
    }

    #[test]
    fn a_fully_typed_syllable_does_not_trigger_abbreviation_matching() {
        // 只要有一個音節打了不只一個符號（正常組字，不是縮寫輸入），
        // 就不該套用縮寫索引。
        let mut engine = Engine::new(abbreviation_dictionary());

        // 你 = ㄋㄧˇ : s(ㄋ) u(ㄧ) 3(ˇ)
        engine.key_press('s');
        engine.key_press('u');
        engine.key_press('3');
        // 再打一個只有聲母的 ㄒ。
        let outcome = engine.key_press('v');
        let KeyOutcome::Composing { candidates, .. } = outcome else {
            panic!("expected Composing outcome");
        };
        assert!(
            candidates.is_empty(),
            "「你」是完整音節，不該讓這組音節被當成縮寫查詢"
        );
    }

    /// 台／太／胎都讀 ㄊㄞ，只是聲調不同，用來驗證「不分聲調選字」
    /// （見模組文件）。
    fn toneless_dictionary() -> Dictionary {
        Dictionary::parse(
            "ㄊㄞˊ\t台\t3000\n\
             ㄊㄞˋ\t太\t5000\n\
             ㄊㄞ\t胎\t500\n\
             ㄏㄠˇ\t好\t9000\n",
        )
    }

    #[test]
    fn typing_a_base_reading_without_a_tone_shows_candidates_across_every_tone() {
        let mut engine = Engine::new(toneless_dictionary());

        // 台 = ㄊㄞˊ : w(ㄊ) 9(ㄞ)，故意不打聲調 6。
        engine.key_press('w');
        let outcome = engine.key_press('9');

        let KeyOutcome::Composing {
            buffer, candidates, ..
        } = outcome
        else {
            panic!("expected Composing outcome");
        };
        assert_eq!(buffer, "ㄊㄞ");
        let mut words: Vec<&str> = candidates.iter().map(|e| e.word.as_str()).collect();
        words.sort();
        assert_eq!(words, vec!["台", "太", "胎"]);
    }

    #[test]
    fn typing_the_tone_narrows_back_down_to_the_exact_match() {
        let mut engine = Engine::new(toneless_dictionary());

        engine.key_press('w');
        engine.key_press('9');
        // 補上聲調 6（ˊ），應該收斂回精確比對，不再混入其他聲調。
        let outcome = engine.key_press('6');
        let KeyOutcome::Composing { candidates, .. } = outcome else {
            panic!("expected Composing outcome");
        };
        assert_eq!(
            candidates,
            vec![Entry {
                word: "台".into(),
                frequency: 3000
            }],
            "打了聲調後應該只精確比對，不該混入「太」「胎」"
        );
    }

    #[test]
    fn toneless_candidate_can_be_selected_like_a_normal_one() {
        let mut engine = Engine::new(toneless_dictionary());
        engine.key_press('w');
        engine.key_press('9');
        let committed = engine.select_candidate("太");
        assert_eq!(committed, "太");
        assert_eq!(engine.buffer(), "");
    }

    #[test]
    fn empty_buffer_does_not_trigger_toneless_matching() {
        let engine = Engine::new(toneless_dictionary());
        assert_eq!(engine.toneless_key(), None);
    }

    #[test]
    fn extends_current_syllable_is_true_for_an_empty_slot() {
        let mut engine = Engine::new(toneless_dictionary());
        // 台 = ㄊㄞˊ : w(ㄊ) 9(ㄞ)，還沒打聲調。
        engine.key_press('w');
        engine.key_press('9');
        assert!(
            engine.extends_current_syllable('6'), // 6 = 聲調 ˊ，目前是空的
            "聲調槽位還空著，聲調鍵應該視為繼續組字"
        );
    }

    #[test]
    fn extends_current_syllable_is_false_once_the_slot_is_filled() {
        let mut engine = Engine::new(toneless_dictionary());
        engine.key_press('w');
        engine.key_press('9');
        engine.key_press('6'); // 補上聲調，ㄊㄞˊ 已完整
        assert!(
            !engine.extends_current_syllable('6'),
            "聲調槽位已經填過，同一個聲調鍵不該再被當成繼續組字"
        );
        assert!(
            !engine.extends_current_syllable('w'), // w = 聲母 ㄊ，聲母槽位也已填過
            "聲母槽位已經填過，不該被當成繼續組字"
        );
    }

    #[test]
    fn extends_current_syllable_is_false_for_a_non_zhuyin_key() {
        let engine = Engine::new(toneless_dictionary());
        assert!(!engine.extends_current_syllable('!'));
    }

    // 使用者自訂詞的注音碼可以是任何打得出來的音節字串，不必對應真實
    // 讀音——底下範例統一用 z(ㄈ) 這個單一聲母當捷徑代碼，打一個鍵就
    // 能叫出候選字（單一聲母就已經 `is_ready()`，不必等第二個音節）。

    #[test]
    fn user_phrase_candidates_take_priority_over_the_dictionary() {
        let dict = Dictionary::parse("ㄈ\t分\t1000\n");
        let mut engine = Engine::new(dict);
        engine
            .add_user_phrase("ㄈ", "台北市大安區羅斯福路四段1號")
            .unwrap();

        let outcome = engine.key_press('z');
        let KeyOutcome::Composing {
            buffer, candidates, ..
        } = outcome
        else {
            panic!("expected Composing outcome");
        };
        assert_eq!(buffer, "ㄈ");
        assert_eq!(
            candidates[0].word, "台北市大安區羅斯福路四段1號",
            "使用者自訂詞應該排在詞庫候選字最前面"
        );
        assert!(candidates.iter().any(|e| e.word == "分"));
    }

    #[test]
    fn selecting_a_user_phrase_works_like_any_other_candidate() {
        let mut engine = Engine::new(Dictionary::new());
        engine.add_user_phrase("ㄈ", "台北市大安區").unwrap();
        engine.key_press('z');
        let committed = engine.select_candidate("台北市大安區");
        assert_eq!(committed, "台北市大安區");
        assert_eq!(engine.buffer(), "");
    }

    #[test]
    fn removing_a_user_phrase_stops_it_from_appearing() {
        let mut engine = Engine::new(Dictionary::new());
        engine.add_user_phrase("ㄈ", "台北市大安區").unwrap();
        assert!(engine.remove_user_phrase("ㄈ", "台北市大安區").unwrap());

        let outcome = engine.key_press('z');
        let KeyOutcome::Composing { candidates, .. } = outcome else {
            panic!("expected Composing outcome");
        };
        assert!(candidates.is_empty());
    }

    #[test]
    fn set_user_phrases_replaces_the_whole_set() {
        let mut engine = Engine::new(Dictionary::new());
        engine.add_user_phrase("ㄈ", "舊地址").unwrap();

        let mut replacement = UserPhrases::new();
        replacement.add("ㄈ", "新地址").unwrap();
        engine.set_user_phrases(replacement);

        let outcome = engine.key_press('z');
        let KeyOutcome::Composing { candidates, .. } = outcome else {
            panic!("expected Composing outcome");
        };
        assert_eq!(
            candidates,
            vec![Entry {
                word: "新地址".into(),
                frequency: user_phrases::USER_PHRASE_FREQUENCY
            }]
        );
    }

    /// 「我」+「是」是詞庫裡的真實詞（較高詞頻），「我」+「市」不是；
    /// 「市」的基礎詞頻故意設得比「是」高，用來驗證 bigram 加權真的會
    /// 蓋過純詞頻排序，而不是恰好詞頻本來就比較高。
    fn bigram_dictionary() -> Dictionary {
        Dictionary::parse(
            "ㄨㄛˇ\t我\t9000\n\
             ㄕˋ\t市\t9000\n\
             ㄕˋ\t是\t100\n\
             ㄨㄛˇ ㄕˋ\t我是\t5000\n",
        )
    }

    #[test]
    fn bigram_context_reorders_candidates_toward_the_word_that_commonly_follows() {
        let mut engine = Engine::new(bigram_dictionary());

        // 我 = ㄨㄛˇ : h(ㄨ)... 用鍵盤查：ㄨ 在 'j'，ㄛ 在 'i'，ˇ 在 '3'。
        engine.key_press('j');
        engine.key_press('i');
        engine.key_press('3');
        let committed = engine.select_candidate("我");
        assert_eq!(committed, "我");

        // 是 = ㄕˋ : g(ㄕ) 4(ˋ)
        engine.key_press('g');
        let outcome = engine.key_press('4');
        let KeyOutcome::Composing { candidates, .. } = outcome else {
            panic!("expected Composing outcome");
        };
        assert_eq!(
            candidates[0].word, "是",
            "「我」後面接「是」有真實的『我是』片語，該蓋過「市」較高的基礎詞頻"
        );
    }

    #[test]
    fn bigram_context_does_not_apply_without_a_prior_committed_word() {
        let mut engine = Engine::new(bigram_dictionary());

        // 沒有先送出任何字，直接打「是」：應該退回純詞頻排序，「市」在前。
        engine.key_press('g');
        let outcome = engine.key_press('4');
        let KeyOutcome::Composing { candidates, .. } = outcome else {
            panic!("expected Composing outcome");
        };
        assert_eq!(candidates[0].word, "市");
    }

    #[test]
    fn reset_context_clears_the_bigram_context() {
        let mut engine = Engine::new(bigram_dictionary());
        engine.key_press('j');
        engine.key_press('i');
        engine.key_press('3');
        engine.select_candidate("我");
        engine.reset_context();

        engine.key_press('g');
        let outcome = engine.key_press('4');
        let KeyOutcome::Composing { candidates, .. } = outcome else {
            panic!("expected Composing outcome");
        };
        assert_eq!(
            candidates[0].word, "市",
            "reset_context 後應該退回純詞頻排序"
        );
    }

    #[test]
    fn backspace_uncommits_the_last_syllable_one_symbol_at_a_time() {
        let mut engine = Engine::new(phrase_dictionary());
        engine.key_press('s');
        engine.key_press('u');
        engine.key_press('3');
        engine.key_press('c');
        engine.key_press('l');
        engine.key_press('3'); // buffer = "ㄋㄧˇㄏㄠˇ", 累積中的「你好」

        let outcome = engine.backspace();
        match outcome {
            KeyOutcome::Composing {
                buffer, candidates, ..
            } => {
                assert_eq!(buffer, "ㄋㄧˇㄏㄠ", "應該只刪掉「好」的聲調，不是整個音節");
                // 「好」的聲調被刪掉後，雖然還不是「ㄏㄠˇ」的精確比對，
                // 但「不分聲調選字」（見模組文件）讓「你好」還是能透過
                // toneless 比對找到，不必重新打一次聲調。
                assert_eq!(
                    candidates,
                    vec![Entry {
                        word: "你好".into(),
                        frequency: 1227
                    }],
                    "刪掉聲調後仍應能靠不分聲調選字找到「你好」"
                );
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
}

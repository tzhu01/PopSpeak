//! Traditional-to-Simplified Chinese post-processor.
//!
//! Whisper models often emit Traditional characters for zh audio even when the
//! speaker uses Mandarin/Simplified. `initial_prompt` biases the decoder toward
//! Simplified, but ~5-10% of tokens still leak through as Traditional. This
//! table cleans the residue after decoding.
//!
//! Coverage: selected single-character mappings used by the speech pipeline.
//! The non-identity mappings derive from OpenCC's Apache-2.0-licensed
//! `TSCharacters.txt` at the exact revision recorded in `t2s_table.txt`, then
//! modified/extracted for this file format; local identity entries are no-ops.
//! Multi-char idiom differences (e.g. 「餐後」/「餐后」handled per-char) are
//! covered by the per-char pass.

use std::collections::HashMap;
use std::sync::OnceLock;

static TABLE: OnceLock<HashMap<char, char>> = OnceLock::new();

fn table() -> &'static HashMap<char, char> {
    TABLE.get_or_init(|| {
        let raw: &str = include_str!("t2s_table.txt");
        let mut m = HashMap::with_capacity(1500);
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut chars = line.chars();
            let (Some(t), Some(s)) = (chars.next(), chars.next()) else {
                continue;
            };
            m.insert(t, s);
        }
        m
    })
}

/// Convert a Traditional-Chinese-flavoured string to Simplified. English,
/// digits, punctuation, and unmapped CJK stay unchanged.
pub fn to_simplified(input: &str) -> String {
    let map = table();
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        out.push(*map.get(&c).unwrap_or(&c));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_ascii() {
        assert_eq!(to_simplified("hello world 123"), "hello world 123");
    }

    #[test]
    fn simplifies_common_chars() {
        // These are all mappings we ship in t2s_table.txt.
        assert_eq!(to_simplified("這個時間"), "这个时间");
        assert_eq!(to_simplified("開發語言"), "开发语言");
        assert_eq!(to_simplified("為什麼會這樣"), "为什么会这样");
    }

    #[test]
    fn mixed_scripts_ok() {
        assert_eq!(
            to_simplified("我在寫 Rust 代碼，執行 cargo build。"),
            "我在写 Rust 代码，执行 cargo build。"
        );
    }
}

//! A bounded vocabulary snapshot for decoder context, not output replacement.
use std::collections::HashSet;

pub fn select_hotwords(words: &[String]) -> Vec<String> {
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    let mut budget = 0;
    for raw in words {
        let word = raw.trim();
        if word.is_empty()
            || word.chars().count() > 32
            || word
                .chars()
                .any(|c| c.is_control() || "<>[]{}|,".contains(c))
            || seen.contains(word)
        {
            continue;
        }
        let cost = 2 + word
            .chars()
            .map(|c| if c.is_ascii() { 1 } else { 2 })
            .sum::<usize>();
        if selected.len() >= 32 || budget + cost > 160 {
            continue;
        }
        budget += cost;
        seen.insert(word.to_string());
        selected.push(word.to_string());
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_rejects_template_or_control_characters() {
        let terms = [
            " PopSpeak ",
            "PopSpeak",
            "张三",
            "[system]",
            "a\nb",
            "<|system|>",
            "x,y",
            "",
        ];
        assert_eq!(
            select_hotwords(&terms.map(String::from)),
            vec!["PopSpeak", "张三"]
        );
    }

    #[test]
    fn matches_frontend_unicode_and_context_budget() {
        let terms = vec![
            "长".repeat(33),
            "中".repeat(32),
            "文".repeat(32),
            "热词".into(),
            "尾".repeat(16),
            "词".into(),
        ];
        assert_eq!(
            select_hotwords(&terms),
            vec!["中".repeat(32), "文".repeat(32), "热词".into(), "词".into()]
        );
    }

    #[test]
    fn edits_and_deletions_do_not_reuse_previous_snapshot() {
        assert_eq!(select_hotwords(&["旧词".into()]), vec!["旧词"]);
        assert_eq!(select_hotwords(&["新词".into()]), vec!["新词"]);
        assert!(select_hotwords(&[]).is_empty());
    }
}

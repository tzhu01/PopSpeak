use pinyin::ToPinyin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotwordRule {
    pub word: String,
    pub pronunciation: Option<String>,
    pub correction_from: Option<String>,
}

/// Apply explicit learned corrections first, then restore the spelling/casing
/// of terms already recognized, and finally use same-pinyin matching for
/// Chinese terms. Everything runs locally on the CPU.
pub fn apply_dictionary(text: &str, rules: &[HotwordRule]) -> String {
    if text.is_empty() || rules.is_empty() {
        return text.to_string();
    }

    let mut sorted = rules.to_vec();
    sorted.sort_by_key(|entry| std::cmp::Reverse(entry.word.chars().count()));
    let mut result = text.to_string();

    for rule in &sorted {
        if let Some(source) = rule
            .correction_from
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            result = replace_case_insensitive(&result, source, &rule.word);
        }
    }

    for rule in &sorted {
        result = restore_normalized_form(&result, &rule.word);
    }

    for rule in &sorted {
        result = replace_same_pinyin(&result, rule);
    }
    result
}

pub fn apply_hotwords(text: &str, hotwords: &[String]) -> String {
    let rules = hotwords
        .iter()
        .map(|word| HotwordRule {
            word: word.clone(),
            pronunciation: None,
            correction_from: None,
        })
        .collect::<Vec<_>>();
    apply_dictionary(text, &rules)
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric() || is_cjk(*character))
        .collect::<String>()
        .to_lowercase()
}

fn is_term_character(character: char) -> bool {
    character.is_alphanumeric() || is_cjk(character)
}

fn normalize_pronunciation(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphabetic())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_cjk(character: char) -> bool {
    matches!(character,
        '\u{4E00}'..='\u{9FFF}' |
        '\u{3400}'..='\u{4DBF}' |
        '\u{20000}'..='\u{2A6DF}'
    )
}

fn pinyin_of(value: &str) -> Option<String> {
    if value.is_empty() || !value.chars().all(is_cjk) {
        return None;
    }
    let syllables = value
        .to_pinyin()
        .map(|item| item.map(|pinyin| pinyin.plain().to_string()))
        .collect::<Option<Vec<_>>>()?;
    Some(syllables.concat().to_lowercase())
}

fn replace_case_insensitive(text: &str, source: &str, replacement: &str) -> String {
    if source.is_empty() {
        return text.to_string();
    }
    let text_chars = text.chars().collect::<Vec<_>>();
    let source_chars = source.chars().collect::<Vec<_>>();
    if source_chars.len() > text_chars.len() {
        return text.to_string();
    }
    let source_lower = source.to_lowercase();
    let mut output = String::new();
    let mut index = 0;
    while index < text_chars.len() {
        if index + source_chars.len() <= text_chars.len() {
            let window = text_chars[index..index + source_chars.len()]
                .iter()
                .collect::<String>();
            if window.to_lowercase() == source_lower {
                output.push_str(replacement);
                index += source_chars.len();
                continue;
            }
        }
        output.push(text_chars[index]);
        index += 1;
    }
    output
}

fn restore_normalized_form(text: &str, target: &str) -> String {
    // Symbol-bearing terms such as C++, .NET and GPT-4 must only match their
    // literal spelling. Stripping their symbols would turn `C++` into the
    // dangerously broad normalized key `c`.
    if target
        .chars()
        .any(|character| !is_term_character(character))
    {
        return replace_case_insensitive(text, target, target);
    }
    let normalized_target = normalize(target);
    if normalized_target.is_empty() || !normalize(text).contains(&normalized_target) {
        return text.to_string();
    }

    let chars = text.chars().collect::<Vec<_>>();
    let target_chars = target.chars().count();
    let mut output = String::with_capacity(text.len());
    let mut start = 0;
    while start < chars.len() {
        // Separators may occur inside a term (`open ai`), but a match must not
        // start on one or it would consume the preceding space/punctuation.
        if !is_term_character(chars[start]) {
            output.push(chars[start]);
            start += 1;
            continue;
        }
        let maximum_end = (start + target_chars + 8).min(chars.len());
        let mut matched_end = None;
        for end in (start + 1)..=maximum_end {
            let window = chars[start..end].iter().collect::<String>();
            let normalized_window = normalize(&window);
            if normalized_window == normalized_target {
                let ascii_target = normalized_target.is_ascii();
                let left_is_word = start > 0 && chars[start - 1].is_ascii_alphanumeric();
                let right_is_word = end < chars.len() && chars[end].is_ascii_alphanumeric();
                if !ascii_target || (!left_is_word && !right_is_word) {
                    matched_end = Some(end);
                }
                break;
            }
            if normalized_window.chars().count() > normalized_target.chars().count() {
                break;
            }
        }
        if let Some(end) = matched_end {
            output.push_str(target);
            start = end;
        } else {
            output.push(chars[start]);
            start += 1;
        }
    }
    output
}

fn replace_same_pinyin(text: &str, rule: &HotwordRule) -> String {
    let target_len = rule.word.chars().count();
    if target_len < 2 || !rule.word.chars().all(is_cjk) {
        return text.to_string();
    }
    // Phonetic replacement is deliberately opt-in. Deriving it for every
    // dictionary word would make unrelated homophones change unexpectedly.
    let target_pinyin = rule
        .pronunciation
        .as_deref()
        .map(normalize_pronunciation)
        .filter(|value| !value.is_empty());
    let Some(target_pinyin) = target_pinyin else {
        return text.to_string();
    };

    let chars = text.chars().collect::<Vec<_>>();
    if target_len > chars.len() {
        return text.to_string();
    }
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if index + target_len <= chars.len() {
            let window = chars[index..index + target_len].iter().collect::<String>();
            if window != rule.word && pinyin_of(&window).as_deref() == Some(target_pinyin.as_str())
            {
                output.push_str(&rule.word);
                index += target_len;
                continue;
            }
        }
        output.push(chars[index]);
        index += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_english_case_without_eating_space() {
        assert_eq!(
            apply_hotwords("openai is great; open ai too", &["OpenAI".to_string()]),
            "OpenAI is great; OpenAI too"
        );
    }

    #[test]
    fn symbol_terms_do_not_expand_into_unrelated_letters() {
        assert_eq!(
            apply_hotwords("C language and c++", &["C++".to_string()]),
            "C language and C++"
        );
    }

    #[test]
    fn ascii_terms_respect_word_boundaries() {
        assert_eq!(
            apply_hotwords("myopenai open ai", &["OpenAI".to_string()]),
            "myopenai OpenAI"
        );
    }

    #[test]
    fn applies_learned_correction() {
        let rules = vec![HotwordRule {
            word: "SenseVoice".to_string(),
            pronunciation: None,
            correction_from: Some("sense voice".to_string()),
        }];
        assert_eq!(
            apply_dictionary("use sense voice now", &rules),
            "use SenseVoice now"
        );
    }

    #[test]
    fn corrects_same_pinyin_chinese_term() {
        let rules = vec![HotwordRule {
            word: "展业".to_string(),
            pronunciation: Some("zhan ye".to_string()),
            correction_from: None,
        }];
        assert_eq!(
            apply_dictionary("展页和展业，再说展页", &rules),
            "展业和展业，再说展业"
        );
    }

    #[test]
    fn longer_terms_are_applied_first() {
        let words = vec!["人工智能".to_string(), "智能".to_string()];
        assert_eq!(apply_hotwords("人工智能", &words), "人工智能");
    }
}

/// Post-process transcribed text to fix Chinese punctuation issues.
///
/// Whisper small model sometimes produces incomplete or English punctuation
/// in Chinese text. This module applies rule-based corrections.
pub fn fix_punctuation(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut result = text.to_string();

    // 1. Replace English punctuation with Chinese equivalents in Chinese context
    result = replace_english_punctuation(&result);

    // 2. Ensure text ends with proper punctuation
    result = ensure_ending_punctuation(&result);

    result
}

/// Replace English punctuation with Chinese equivalents
fn replace_english_punctuation(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        let prev_is_cjk = i > 0 && is_cjk(chars[i - 1]);
        let next_is_cjk = i + 1 < chars.len() && is_cjk(chars[i + 1]);
        let in_chinese_context = prev_is_cjk || next_is_cjk;

        if in_chinese_context {
            match ch {
                ',' => result.push('\u{FF0C}'), // ，
                '.' => result.push('\u{3002}'), // 。
                '?' => result.push('\u{FF1F}'), // ？
                '!' => result.push('\u{FF01}'), // ！
                ':' => result.push('\u{FF1A}'), // ：
                ';' => result.push('\u{FF1B}'), // ；
                '(' => result.push('\u{FF08}'), // （
                ')' => result.push('\u{FF09}'), // ）
                _ => result.push(ch),
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Ensure text ends with appropriate punctuation
fn ensure_ending_punctuation(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return text.to_string();
    }

    let last_char = trimmed.chars().last().unwrap();

    // Already has ending punctuation
    if is_ending_punctuation(last_char) {
        return trimmed.to_string();
    }

    // Check if it looks like a question
    let is_question = contains_question_words(trimmed);

    let mut result = trimmed.to_string();
    let last_is_cjk = is_cjk(last_char);

    if is_question {
        result.push(if last_is_cjk { '\u{FF1F}' } else { '?' });
    } else {
        result.push(if last_is_cjk { '\u{3002}' } else { '.' });
    }

    result
}

fn is_cjk(ch: char) -> bool {
    let cp = ch as u32;
    // CJK Unified Ideographs and common CJK ranges
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x2E80..=0x2EFF).contains(&cp)
        || (0x3000..=0x303F).contains(&cp)
        || (0xFF00..=0xFFEF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
}

fn is_ending_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '.' | '。'
            | '?'
            | '？'
            | '!'
            | '！'
            | '…'
            | '"'
            | '\u{201D}' // "
            | '\u{300D}' // 」
    )
}

fn contains_question_words(text: &str) -> bool {
    let question_words = [
        "吗", "呢", "么", "什么", "怎么", "为什么", "哪", "哪里", "哪个", "几", "多少", "是否",
        "能否", "可否", "是不是", "对不对", "好不好",
    ];
    question_words.iter().any(|w| text.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_english_to_chinese_punctuation() {
        assert_eq!(fix_punctuation("你好,世界"), "你好\u{FF0C}世界\u{3002}");
    }

    #[test]
    fn test_question_detection() {
        let result = fix_punctuation("你好吗");
        assert!(result.ends_with('？'));
    }

    #[test]
    fn test_already_has_punctuation() {
        assert_eq!(fix_punctuation("你好。"), "你好。");
    }

    #[test]
    fn test_empty() {
        assert_eq!(fix_punctuation(""), "");
    }

    #[test]
    fn test_english_text_preserved() {
        let result = fix_punctuation("hello world");
        assert_eq!(result, "hello world.");
    }
}

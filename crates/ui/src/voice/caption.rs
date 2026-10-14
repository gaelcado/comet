//! The dock's live caption: the end of what is being said, on one line.
use super::VoiceController;
use zeron_proto::voice::VoiceRole;

/// The end of `text` holding at most `max` characters, starting on a whole
/// word: the host clips the top, so nothing marks the cut.
pub(crate) fn tail(text: &str, max: usize) -> &str {
    let text = text.trim();
    let count = text.chars().count();
    if count <= max {
        return text;
    }
    let start = text
        .char_indices()
        .nth(count - max)
        .map_or(0, |(index, _)| index);
    let cut = &text[start..];
    if text[..start].ends_with(char::is_whitespace) || cut.starts_with(char::is_whitespace) {
        return cut.trim_start();
    }
    match cut.find(char::is_whitespace) {
        Some(space) => cut[space..].trim_start(),
        None => cut,
    }
}

/// Byte offset where the last `words` words of `text` begin: the part of a
/// caption that is being said right now. Zero when it has no more than that.
pub(crate) fn last_words(text: &str, words: usize) -> usize {
    let mut seen = 0;
    let mut in_word = false;
    for (index, c) in text.char_indices().rev() {
        if c.is_whitespace() {
            if in_word {
                seen += 1;
                if seen == words {
                    return index + c.len_utf8();
                }
            }
            in_word = false;
        } else {
            in_word = true;
        }
    }
    0
}

/// The current utterance's latest words and who is saying them, or `None`
/// while nothing has been said.
pub fn caption(voice: &VoiceController, max_chars: usize) -> Option<(String, Option<VoiceRole>)> {
    let current = voice.current()?;
    Some((tail(&current.text, max_chars).to_owned(), current.role))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_words_finds_where_the_newest_words_begin() {
        let text = "one two  three four";
        assert_eq!(&text[last_words(text, 1)..], "four");
        assert_eq!(&text[last_words(text, 2)..], "three four");
        // Trailing space belongs to the word before it.
        assert_eq!(&"one two "[last_words("one two ", 1)..], "two ");
        // Fewer words than asked for: all of it.
        assert_eq!(last_words(text, 9), 0);
        assert_eq!(last_words("", 3), 0);
        assert_eq!(&"çà là déjà"[last_words("çà là déjà", 2)..], "là déjà");
    }

    #[test]
    fn tail_keeps_the_latest_whole_words() {
        assert_eq!(tail("  short  ", 20), "short");
        let long = "one two three four five six seven";
        assert_eq!(tail(long, 14), "five six seven");
        // A cut inside a word drops the fragment.
        assert_eq!(tail(long, 13), "six seven");
        // A single unbroken run still fits the budget.
        assert_eq!(tail("abcdefghij", 4), "ghij");
        // Character budgets, never byte offsets.
        assert_eq!(tail("héllo wörld ünï", 9), "wörld ünï");
    }
}

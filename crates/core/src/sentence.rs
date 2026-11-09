//! Small, local sentence-boundary detector used by hover extraction.
//!
//! UI Automation exposes word, line, and paragraph units, but not sentences.
//! This module deliberately works on the bounded text returned by an adapter;
//! it never performs I/O and treats a paragraph without punctuation as one
//! sentence.

const ABBREVIATIONS: &[&str] = &[
    "mr.", "mrs.", "ms.", "dr.", "prof.", "sr.", "jr.", "st.", "vs.", "etc.", "e.g.", "i.e.",
    "fig.", "no.", "u.s.", "u.k.",
];

/// Return the sentence containing `target` in `context`.
pub fn sentence_for_target(context: &str, target: &str) -> Option<String> {
    let start = find_target(context, target)?;
    sentence_for_target_at(context, target, start)
}

/// Return the sentence containing a target at an explicitly supplied byte
/// offset. The offset must identify the requested occurrence in `context`;
/// this prevents repeated words from silently resolving to the first match.
///
/// The offset is a Rust UTF-8 byte offset. Adapters that receive UTF-16 text
/// should convert their range prefix to a Rust byte offset before calling it.
pub fn sentence_for_target_at(context: &str, target: &str, target_offset: usize) -> Option<String> {
    let needle = target.trim();
    if needle.is_empty() || !context.is_char_boundary(target_offset) {
        return None;
    }
    let candidate = context.get(target_offset..)?.get(..needle.len())?;
    if crate::normalize::normalize_target(candidate) != crate::normalize::normalize_target(needle) {
        return None;
    }
    let (left, right) = sentence_bounds(context, target_offset);
    let sentence = context.get(left..right)?.trim();
    (!sentence.is_empty()).then(|| sentence.to_owned())
}

/// Alias with a descriptive name for platform adapters.
pub fn extract_sentence_context(context: &str, target: &str) -> Option<String> {
    sentence_for_target(context, target)
}

/// Longest context kept, in Unicode scalars (about three lines of the popup's
/// quotation). A longer sentence is narrowed to clauses, then words, around
/// the target; the target itself is never cut.
pub const MAX_SELECTION_CONTEXT_SCALARS: usize = 160;

/// A selection this long already carries its own context; surrounding text
/// would only add noise (and tokens) to the request.
pub const MAX_CONTEXTUAL_TARGET_SCALARS: usize = 120;

/// The context worth sending with a Selection/Manual target: the ONE
/// sentence (on the target's own line) that contains the whole, unchanged
/// target — or nothing.
///
/// No context is returned when the target spans several sentences or lines,
/// is long enough to stand alone, or is (nearly) the whole sentence already.
/// Line breaks are hard boundaries: headings, list items, labels, and code
/// lines without terminal punctuation used to merge into one huge
/// "sentence" that swamped the popup and the prompt.
pub fn selection_context(context: &str, target: &str) -> Option<String> {
    let start = find_target(context, target)?;
    selection_context_at(context, target, start)
}

/// False when [`selection_context`] could never return context for this
/// target (too long, several lines, or several sentences), so callers can
/// skip work such as an OCR enrichment pass.
pub fn selection_target_wants_context(target: &str) -> bool {
    let needle = target.trim();
    !needle.is_empty()
        && needle.chars().count() < MAX_CONTEXTUAL_TARGET_SCALARS
        && !needle.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
        && !spans_sentences(needle)
}

/// [`selection_context`] for an explicitly identified occurrence (a UTF-8
/// byte offset into `context`), so repeated words resolve correctly.
pub fn selection_context_at(context: &str, target: &str, target_offset: usize) -> Option<String> {
    let needle = target.trim();
    if !selection_target_wants_context(needle) {
        return None;
    }
    let (sentence, start, end) = line_sentence_at(context, needle, target_offset)?;
    let meaningful = |text: &str| text.chars().filter(|c| c.is_alphanumeric()).count();
    if meaningful(sentence) < meaningful(needle) + 3 {
        // Nothing but punctuation or a word or two beyond the selection.
        return None;
    }
    Some(fit_context(sentence, start, end))
}

/// Hover context: the sentence on the pointed word's own line, bounded the
/// same way as Selection context. Unlike Selection it is kept even when it
/// adds little (Hover requires a containing sentence).
pub fn hover_context_at(context: &str, target: &str, target_offset: usize) -> Option<String> {
    let needle = target.trim();
    if needle.is_empty() {
        return None;
    }
    let (sentence, start, end) = line_sentence_at(context, needle, target_offset)?;
    Some(fit_context(sentence, start, end))
}

/// The sentence containing `needle` at `target_offset`, bounded by the
/// target's own line (headings, list items, labels, and code lines carry no
/// terminal punctuation and used to merge into one huge "sentence"). Returns
/// the trimmed sentence and the target's byte range inside it.
fn line_sentence_at<'a>(
    context: &'a str,
    needle: &str,
    target_offset: usize,
) -> Option<(&'a str, usize, usize)> {
    if needle.is_empty() || !context.is_char_boundary(target_offset) {
        return None;
    }
    let candidate = context.get(target_offset..)?.get(..needle.len())?;
    if crate::normalize::normalize_target(candidate) != crate::normalize::normalize_target(needle) {
        return None;
    }
    let target_end = target_offset + needle.len();
    let line_start = context[..target_offset]
        .rfind(is_line_break)
        .map_or(0, |index| {
            index + context[index..].chars().next().map_or(1, char::len_utf8)
        });
    let line_end = context[target_end..]
        .find(is_line_break)
        .map_or(context.len(), |index| target_end + index);
    let line = &context[line_start..line_end];
    let local_start = target_offset - line_start;
    let local_end = target_end - line_start;
    let last_char = line[..local_end]
        .char_indices()
        .next_back()
        .map_or(local_start, |(index, _)| index);
    let left = sentence_bounds(line, local_start).0.min(local_start);
    let right = sentence_bounds(line, last_char).1.max(local_end);
    let raw = &line[left..right];
    let lead = raw.len() - raw.trim_start().len();
    let sentence = raw.trim();
    let start = (local_start - left).checked_sub(lead)?;
    let end = start + needle.len();
    (sentence.get(start..end) == Some(candidate)).then_some((sentence, start, end))
}

fn is_line_break(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Clause punctuation: a long sentence is narrowed to whole clauses around
/// the target before any word-level cut.
fn is_clause_mark(ch: char) -> bool {
    matches!(
        ch,
        ',' | ';' | ':' | '，' | '；' | '：' | '、' | '—' | '–' | '(' | ')' | '（' | '）'
    )
}

/// True when a sentence boundary falls inside the target (terminal
/// punctuation at its very end does not count).
fn spans_sentences(target: &str) -> bool {
    let trimmed = target.trim_end_matches(|c: char| {
        matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '；' | ';') || is_closing_quote(c)
    });
    trimmed
        .char_indices()
        .any(|(index, ch)| is_boundary(trimmed, index, ch) && index + ch.len_utf8() < trimmed.len())
}

/// Fit a sentence into [`MAX_SELECTION_CONTEXT_SCALARS`] without touching
/// the target at `[start, end)`: a short sentence is kept whole; a long one
/// keeps the clause holding the target plus as many neighbouring clauses as
/// fit; a single over-long clause falls back to a word window. Every cut is
/// marked with an ellipsis.
fn fit_context(sentence: &str, start: usize, end: usize) -> String {
    let scalars = |text: &str| text.trim().chars().count();
    if scalars(sentence) <= MAX_SELECTION_CONTEXT_SCALARS {
        return sentence.to_owned();
    }
    // Cut points sit just after each clause mark, outside the target.
    let cuts: Vec<usize> = sentence
        .char_indices()
        .filter(|(_, ch)| is_clause_mark(*ch))
        .map(|(index, ch)| index + ch.len_utf8())
        .filter(|cut| *cut <= start || *cut >= end)
        .collect();
    let mut lefts: Vec<usize> = cuts.iter().copied().filter(|cut| *cut <= start).collect();
    lefts.reverse();
    let rights: Vec<usize> = cuts.iter().copied().filter(|cut| *cut >= end).collect();
    let (mut li, mut ri) = (0usize, 0usize);
    let mut left = lefts.first().copied().unwrap_or(0);
    let mut right = rights.first().copied().unwrap_or(sentence.len());
    if scalars(&sentence[left..right]) > MAX_SELECTION_CONTEXT_SCALARS {
        return trim_around(sentence, start, end);
    }
    loop {
        let mut grew = false;
        // Prefer the clause after the target (it usually completes the
        // phrase), then the one before.
        let next_right = rights.get(ri + 1).copied().unwrap_or(sentence.len());
        if right < sentence.len()
            && scalars(&sentence[left..next_right]) <= MAX_SELECTION_CONTEXT_SCALARS
        {
            right = next_right;
            ri += 1;
            grew = true;
        }
        let next_left = lefts.get(li + 1).copied().unwrap_or(0);
        if left > 0 && scalars(&sentence[next_left..right]) <= MAX_SELECTION_CONTEXT_SCALARS {
            left = next_left;
            li += 1;
            grew = true;
        }
        if !grew {
            break;
        }
    }
    let mut body = sentence[left..right].trim();
    if right < sentence.len() {
        body = body.trim_end_matches(is_clause_mark).trim_end();
    }
    let mut out = String::new();
    if left > 0 {
        out.push('…');
    }
    out.push_str(body);
    if right < sentence.len() {
        out.push('…');
    }
    out
}

/// Cut an over-long clause to a window of about
/// [`MAX_SELECTION_CONTEXT_SCALARS`] around `[start, end)`, at word
/// boundaries where the text has spaces, marking each cut with an ellipsis.
fn trim_around(sentence: &str, start: usize, end: usize) -> String {
    let target_scalars = sentence[start..end].chars().count();
    let budget = MAX_SELECTION_CONTEXT_SCALARS.saturating_sub(target_scalars) / 2;
    let before: Vec<(usize, char)> = sentence[..start].char_indices().collect();
    let mut left = before
        .len()
        .checked_sub(budget)
        .map_or(0, |index| before[index].0);
    if left > 0 {
        // Start at the next word boundary so no word is cut in half.
        if let Some(space) = sentence[left..start].find(char::is_whitespace) {
            left += space;
        }
    }
    let mut right = sentence[end..]
        .char_indices()
        .nth(budget)
        .map_or(sentence.len(), |(index, _)| end + index);
    if right < sentence.len() {
        if let Some(space) = sentence[end..right].rfind(char::is_whitespace) {
            right = end + space;
        }
    }
    let mut out = String::new();
    if left > 0 {
        out.push('…');
    }
    out.push_str(sentence[left..right].trim());
    if right < sentence.len() {
        out.push('…');
    }
    out
}

/// Find the byte range of the sentence containing `offset`.
pub fn sentence_bounds(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let mut left = 0;
    for (index, ch) in text.char_indices() {
        if index >= offset {
            break;
        }
        if is_boundary(text, index, ch) {
            left = index + ch.len_utf8();
            while text[left..]
                .chars()
                .next()
                .is_some_and(|next| is_closing_quote(next) || next.is_whitespace())
            {
                left += text[left..].chars().next().map_or(0, char::len_utf8);
            }
        }
    }

    let mut right = text.len();
    for (index, ch) in text.char_indices().filter(|(index, _)| *index >= offset) {
        if is_boundary(text, index, ch) {
            right = index + ch.len_utf8();
            while right < text.len() {
                let next = text[right..].chars().next().expect("valid boundary");
                if !is_closing_quote(next) {
                    break;
                }
                right += next.len_utf8();
            }
            break;
        }
    }
    (left, right)
}

fn find_target(source: &str, target: &str) -> Option<usize> {
    let needle = target.trim();
    if needle.is_empty() {
        return None;
    }
    source.find(needle).or_else(|| {
        needle.is_ascii().then(|| {
            source.char_indices().find_map(|(index, _)| {
                source[index..]
                    .get(..needle.len())
                    .filter(|candidate| candidate.eq_ignore_ascii_case(needle))
                    .map(|_| index)
            })
        })?
    })
}

fn is_boundary(text: &str, index: usize, ch: char) -> bool {
    if !matches!(ch, '.' | '!' | '?' | '。' | '！' | '？' | '；' | ';') {
        return false;
    }
    if ch == '.' && is_abbreviation(text, index) {
        return false;
    }
    true
}

fn is_abbreviation(text: &str, index: usize) -> bool {
    let end = index + 1;
    let start = text[..end]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace() || matches!(c, '(' | '[' | '"' | '\''))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let token = text[start..end].to_lowercase();
    if ABBREVIATIONS.contains(&token.as_str())
        || token.chars().all(|c| c.is_ascii_uppercase() || c == '.')
    {
        return true;
    }
    text[end..]
        .chars()
        .next()
        .is_some_and(|next| next.is_ascii_lowercase())
}

fn is_closing_quote(ch: char) -> bool {
    matches!(
        ch,
        '"' | '\'' | ')' | ']' | '}' | '”' | '’' | '»' | '』' | '」' | '》' | '）' | '］' | '｝'
    )
}

#[cfg(test)]
mod tests {
    use super::sentence_for_target_at;
    use super::{sentence_bounds, sentence_for_target};

    #[test]
    fn english_boundary_and_quotes() {
        assert_eq!(
            sentence_for_target("One starts. Two ends!", "Two"),
            Some("Two ends!".into())
        );
        assert_eq!(
            sentence_for_target("He said, \"Run fast!\" Then stop.", "Run"),
            Some("He said, \"Run fast!\"".into())
        );
    }

    #[test]
    fn abbreviations_do_not_split() {
        assert_eq!(
            sentence_for_target("Dr. Smith arrived. Next step.", "Smith"),
            Some("Dr. Smith arrived.".into())
        );
        assert_eq!(
            sentence_for_target("Use e.g. this value. Done.", "this"),
            Some("Use e.g. this value.".into())
        );
    }

    #[test]
    fn chinese_and_missing_punctuation() {
        assert_eq!(
            sentence_for_target("你好。世界！", "世界"),
            Some("世界！".into())
        );
        assert_eq!(
            sentence_for_target("没有句号的整段文字", "整段"),
            Some("没有句号的整段文字".into())
        );
    }

    #[test]
    fn bounds_are_safe_at_utf8_offsets() {
        let text = "甲。乙";
        assert_eq!(
            &text[sentence_bounds(text, 4).0..sentence_bounds(text, 4).1],
            "乙"
        );
    }

    #[test]
    fn missing_target_does_not_default_to_first_sentence() {
        assert_eq!(
            sentence_for_target("First sentence. Second sentence.", "absent"),
            None
        );
    }

    #[test]
    fn explicit_offset_selects_the_repeated_occurrence() {
        let text = "word first. word second.";
        let second = text.rfind("word").expect("second occurrence");
        assert_eq!(
            sentence_for_target_at(text, "word", second),
            Some("word second.".into())
        );
        assert_eq!(
            sentence_for_target_at(text, "word", text.find("word").unwrap()),
            Some("word first.".into())
        );
    }

    #[test]
    fn non_bmp_target_offsets_are_utf8_safe() {
        let text = "Prefix. 😀 target here.";
        let offset = text.find("😀").expect("emoji");
        assert_eq!(
            sentence_for_target_at(text, "😀", offset),
            Some("😀 target here.".into())
        );
        assert_eq!(sentence_for_target_at(text, "😀", offset + 1), None);
    }

    use super::{selection_context, MAX_SELECTION_CONTEXT_SCALARS};

    #[test]
    fn selection_context_is_the_one_sentence_with_the_target() {
        assert_eq!(
            selection_context("First one. The change applies now. Last one.", "change"),
            Some("The change applies now.".into())
        );
    }

    #[test]
    fn line_breaks_bound_selection_context() {
        let page = "Settings\nProvider\nThe resident reloads settings when the file changes\nModel";
        assert_eq!(
            selection_context(page, "reloads"),
            Some("The resident reloads settings when the file changes".into())
        );
        // A heading or label alone on its line has nothing worth adding.
        assert_eq!(selection_context(page, "Provider"), None);
    }

    #[test]
    fn whole_sentence_or_multi_sentence_targets_get_no_context() {
        let text = "One starts here. Two ends there!";
        assert_eq!(selection_context(text, "One starts here."), None);
        assert_eq!(selection_context(text, "One starts here"), None);
        assert_eq!(selection_context(text, "here. Two"), None);
        assert_eq!(selection_context("a\nb c d e f", "a\nb"), None);
    }

    #[test]
    fn long_targets_stand_alone() {
        let target = "word ".repeat(30);
        let text = format!("Before. {target} after the long selection.");
        assert_eq!(selection_context(&text, target.trim()), None);
    }

    #[test]
    fn long_sentences_are_trimmed_around_the_unchanged_target() {
        let filler = "lorem ipsum dolor sit amet ".repeat(40);
        let text = format!("{filler}the selected term {filler}");
        let context = selection_context(&text, "selected").expect("context");
        assert!(context.contains("the selected term"));
        assert!(context.starts_with('…') && context.ends_with('…'));
        assert!(context.chars().count() <= MAX_SELECTION_CONTEXT_SCALARS + 2);
    }

    #[test]
    fn chinese_selection_context() {
        assert_eq!(
            selection_context("第一句。这里是选中的词语所在的句子！最后。", "选中"),
            Some("这里是选中的词语所在的句子！".into())
        );
    }

    #[test]
    fn long_sentences_keep_whole_clauses_around_the_target() {
        let sentence = "When the resident starts it reads the configuration file, \
            then it opens the history database in the local application data folder, \
            and after the first selection it loads the companion that renders the popup, \
            which keeps a small pool of windows for later requests.";
        let context = selection_context(sentence, "history database").expect("context");
        assert!(context.contains("then it opens the history database"));
        assert!(context.chars().count() <= MAX_SELECTION_CONTEXT_SCALARS + 2);
        assert!(context.starts_with('…') || context.starts_with("When"));
        assert!(!context.trim_end_matches('…').ends_with(','));
    }

    #[test]
    fn hover_context_is_bounded_and_line_scoped() {
        use super::hover_context_at;
        let page = format!(
            "Heading\n{}the word here {}\nNext",
            "alpha ".repeat(60),
            "beta ".repeat(60)
        );
        let offset = page.find("word").unwrap();
        let context = hover_context_at(&page, "word", offset).expect("context");
        assert!(context.contains("the word here"));
        assert!(!context.contains("Heading") && !context.contains("Next"));
        assert!(context.chars().count() <= MAX_SELECTION_CONTEXT_SCALARS + 2);
        // A short line is kept whole, even when it adds little.
        assert_eq!(
            hover_context_at("one\nword\ntwo", "word", 4),
            Some("word".into())
        );
    }
}

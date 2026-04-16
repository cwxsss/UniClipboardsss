//! Deterministic text tokenizer for search indexing.
//!
//! Normalization and tokenization rules (applied in order):
//! 1. NFKC Unicode normalization
//! 2. Lowercase
//! 3. Split on Unicode word boundaries (via `unicode-segmentation`)
//! 4. Further split on `_`, `-`, `.`, `/`
//! 5. Split camelCase and PascalCase boundaries
//! 6. For identifier/path-like inputs, preserve the original normalized whole segment plus parts
//! 7. Drop single-character Latin tokens (keep CJK characters)
//! 8. Generate overlapping bigrams over contiguous CJK runs

use std::collections::HashSet;

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// Stateless deterministic tokenizer.
///
/// Call `tokenize_segment()` or `tokenize_all()` to produce normalized tokens.
pub struct SearchTokenizer;

impl SearchTokenizer {
    /// Tokenize a single raw segment and return all normalized tokens.
    ///
    /// Includes prefix expansion (length 3..N-1) for word-level tokens so that
    /// partial queries like `"uniclip"` match `"uniclipboard"`. Use this for
    /// identifier-rich fields: file names, file paths, URLs.
    ///
    /// For large free-text body fields, prefer [`tokenize_segment_no_prefix`] to
    /// avoid the O(unique_tokens × text_length) cost from `count_raw_tokens`.
    pub fn tokenize_segment(&self, raw: &str) -> Vec<String> {
        self.tokenize_segment_inner(raw, true)
    }

    /// Tokenize a single raw segment **without** prefix expansion.
    ///
    /// Identical to [`tokenize_segment`] but skips the prefix-expansion step.
    /// Use this for body / HTML fields where the text can be large and prefix
    /// matching offers little UX value compared to its indexing cost.
    pub fn tokenize_segment_no_prefix(&self, raw: &str) -> Vec<String> {
        self.tokenize_segment_inner(raw, false)
    }

    fn tokenize_segment_inner(&self, raw: &str, with_prefixes: bool) -> Vec<String> {
        if raw.is_empty() {
            return vec![];
        }

        // Step 1 & 2: NFKC normalize and lowercase the ORIGINAL for whole-segment preservation.
        let nfkc_original: String = raw.nfkc().collect();
        let lowered = nfkc_original.to_lowercase();

        let is_identifier_like = contains_separator(&lowered) || has_camel_case(&nfkc_original);

        let mut candidate_tokens: Vec<String> = Vec::new();

        // Preserve the normalized whole segment for identifier/path-like inputs.
        if is_identifier_like {
            candidate_tokens.push(lowered.clone());
        }

        // Step 3: Split on Unicode word boundaries (already on lowercased form).
        // This handles natural language splitting. Note: unicode_words() skips separator chars.
        for word in lowered.unicode_words() {
            if !word.is_empty() {
                candidate_tokens.push(word.to_string());
            }
        }

        // Step 4 & 5: Separator + camelCase splitting on the original NFKC form.
        // Apply camelCase split first, then separator split on each part.
        let camel_parts = split_camel_case_original(&nfkc_original);
        for camel_part in &camel_parts {
            // Apply separator splitting to each camelCase part
            for sep_part in split_on_separators(camel_part) {
                let lowered_part = sep_part.to_lowercase();
                if !lowered_part.is_empty() {
                    candidate_tokens.push(lowered_part);
                }
            }
        }

        // Also apply separator splitting directly to the lowered form to catch any missed tokens.
        for sep_part in split_on_separators(&lowered) {
            if !sep_part.is_empty() {
                candidate_tokens.push(sep_part.to_string());
            }
        }

        // Step 7: Filter — drop single-character Latin tokens.
        let filtered: Vec<String> = candidate_tokens
            .into_iter()
            .filter(|t| should_keep_token(t))
            .collect();

        // Step 8: Dedup and CJK bigrams.
        let mut result: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for tok in &filtered {
            if seen.insert(tok.clone()) {
                let bigrams = cjk_bigrams(tok);
                if bigrams.is_empty() {
                    result.push(tok.clone());
                } else {
                    // CJK token: expand to bigrams
                    for bg in bigrams {
                        if seen.insert(bg.clone()) {
                            result.push(bg);
                        }
                    }
                }
            }
        }

        // Add CJK bigrams from the full lowered string (catches multi-char CJK runs).
        for bg in cjk_bigrams(&lowered) {
            if seen.insert(bg.clone()) {
                result.push(bg);
            }
        }

        // Prefix expansion: for every word-level token (no separators, non-CJK),
        // emit all prefixes of length 3..(token_len - 1) so that partial queries
        // like "uniclip" match entries indexed under "uniclipboard".
        // Skipped for body/html fields (with_prefixes = false) to avoid O(M×N) blowup.
        if with_prefixes {
            let base_tokens: Vec<String> = result.clone();
            for tok in &base_tokens {
                if !contains_separator(tok) && cjk_bigrams(tok).is_empty() {
                    for prefix in prefix_tokens(tok) {
                        if seen.insert(prefix.clone()) {
                            result.push(prefix);
                        }
                    }
                }
            }
        }

        result
    }

    /// Tokenize multiple raw segments and return a deduplicated flat list.
    ///
    /// Includes prefix expansion. Use for identifier-rich index fields (file names,
    /// paths, URLs). For query-time tokenization use [`tokenize_all_no_prefix`].
    pub fn tokenize_all(&self, raw_segments: &[String]) -> Vec<String> {
        self.tokenize_all_inner(raw_segments, true)
    }

    /// Tokenize multiple raw segments without prefix expansion.
    ///
    /// Use at query time: the user's partial term (e.g. `"uniclip"`) is looked up
    /// as an exact token, matching the prefix tokens stored at index time.
    pub fn tokenize_all_no_prefix(&self, raw_segments: &[String]) -> Vec<String> {
        self.tokenize_all_inner(raw_segments, false)
    }

    fn tokenize_all_inner(&self, raw_segments: &[String], with_prefixes: bool) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut result = Vec::new();

        for seg in raw_segments {
            for tok in self.tokenize_segment_inner(seg, with_prefixes) {
                if seen.insert(tok.clone()) {
                    result.push(tok);
                }
            }
        }

        result
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Private helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Check whether a character is a separator for identifier splitting.
fn is_separator(ch: char) -> bool {
    matches!(ch, '_' | '-' | '.' | '/')
}

/// Check if the string contains any separator characters.
fn contains_separator(s: &str) -> bool {
    s.chars().any(is_separator)
}

/// Split a string on separator characters `_`, `-`, `.`, `/`.
fn split_on_separators(s: &str) -> Vec<&str> {
    s.split(|c: char| is_separator(c))
        .filter(|p| !p.is_empty())
        .collect()
}

/// Check whether the original string has camelCase transitions
/// (lowercase letter followed by uppercase letter).
fn has_camel_case(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    for i in 1..chars.len() {
        if chars[i - 1].is_lowercase() && chars[i].is_uppercase() {
            return true;
        }
    }
    false
}

/// Split camelCase/PascalCase on the original (NFKC but not lowercased) string.
///
/// Handles:
/// - "fooBar"     → ["foo", "Bar"]
/// - "getUserName"→ ["get", "User", "Name"]
/// - "HTMLParser" → ["HTML", "Parser"]
///
/// Each returned part retains its original casing; callers must lowercase.
fn split_camel_case_original(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut result: Vec<String> = Vec::new();
    let mut current = String::new();

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];

        if ch.is_uppercase() && !current.is_empty() {
            let prev_lower = current.chars().last().map_or(false, |c| c.is_lowercase());
            let next_lower = chars.get(i + 1).map_or(false, |c| c.is_lowercase());
            let prev_upper = current.chars().last().map_or(false, |c| c.is_uppercase());

            // Transition: lowercase→UPPER or upper-run before UPPER+lower
            if prev_lower || (prev_upper && next_lower && current.len() > 1) {
                // When prev is all upper and we're about to start a new uppercase+lower word
                // (e.g., "HTMLParser": flush "HTM" keep "L", then "Parser" starts)
                if prev_upper && next_lower && current.len() > 1 {
                    // Keep the last char of current to start the new word
                    let last_char = current.pop().unwrap();
                    result.push(current.clone());
                    current.clear();
                    current.push(last_char);
                } else {
                    result.push(current.clone());
                    current.clear();
                }
            }
        }

        current.push(ch);
        i += 1;
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

/// Generate all prefixes of length 3..(token_len - 1) for a word-level token.
///
/// Returns an empty vec for tokens with fewer than 4 characters (no useful
/// prefix shorter than the token itself can be generated at min-length 3).
fn prefix_tokens(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 3 {
        return vec![];
    }
    // lengths 3, 4, ..., chars.len()-1  (full token already in result)
    (3..chars.len())
        .map(|end| chars[..end].iter().collect())
        .collect()
}

/// Return true if the token should be kept in the output.
///
/// - Drop single-character tokens that are ASCII letters (Latin).
/// - Keep single CJK characters (they contribute to bigrams).
/// - Keep multi-character tokens.
fn should_keep_token(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() == 1 {
        let ch = chars[0];
        // Drop single ASCII alphabetic characters
        if ch.is_ascii_alphabetic() {
            return false;
        }
        // Keep CJK — they form bigrams
        if is_cjk(ch) {
            return true;
        }
        // Keep ASCII digits
        if ch.is_ascii_digit() {
            return true;
        }
        // Drop everything else that's a single char
        return false;
    }
    true
}

/// Check whether a character is in a CJK Unicode range.
fn is_cjk(ch: char) -> bool {
    let c = ch as u32;
    // CJK Unified Ideographs
    (0x4E00..=0x9FFF).contains(&c)
        // CJK Extension A
        || (0x3400..=0x4DBF).contains(&c)
        // CJK Compatibility Ideographs
        || (0xF900..=0xFAFF).contains(&c)
        // CJK Unified Ideographs Extension B
        || (0x20000..=0x2A6DF).contains(&c)
        // Hiragana
        || (0x3040..=0x309F).contains(&c)
        // Katakana
        || (0x30A0..=0x30FF).contains(&c)
        // Hangul Syllables
        || (0xAC00..=0xD7AF).contains(&c)
}

/// Generate overlapping bigrams from contiguous CJK runs in the string.
///
/// For `"中文测试"` produces `["中文", "文测", "测试"]`.
fn cjk_bigrams(s: &str) -> Vec<String> {
    let mut bigrams = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut run_start: Option<usize> = None;

    for (i, &ch) in chars.iter().enumerate() {
        if is_cjk(ch) {
            if run_start.is_none() {
                run_start = Some(i);
            }
        } else if let Some(start) = run_start.take() {
            let run = &chars[start..i];
            for window in run.windows(2) {
                bigrams.push(window.iter().collect());
            }
        }
    }

    // Handle run that extends to end of string
    if let Some(start) = run_start {
        let run = &chars[start..];
        for window in run.windows(2) {
            bigrams.push(window.iter().collect());
        }
    }

    bigrams
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

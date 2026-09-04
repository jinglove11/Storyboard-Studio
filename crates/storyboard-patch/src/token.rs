/// Whole-token aware substring replacement.
///
/// A token matches when its occurrence is not glued to other alphanumeric
/// characters (word-boundary style), so `miku` does not match inside
/// `mikuchan`. Everything else — weights, punctuation, spacing around the
/// token — is preserved byte-for-byte.

fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

/// Count whole-token occurrences of `token` in `text`.
pub fn count_occurrences(text: &str, token: &str) -> usize {
    find_occurrences(text, token).len()
}

pub fn find_occurrences(text: &str, token: &str) -> Vec<usize> {
    if token.is_empty() {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let tb = token.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + tb.len() <= bytes.len() {
        if &bytes[i..i + tb.len()] == tb {
            let before_ok = i == 0 || !text[..i].chars().next_back().map(is_word).unwrap_or(false);
            let after_char = text[i + tb.len()..].chars().next();
            let after_ok = after_char.map(|c| !is_word(c)).unwrap_or(true);
            if before_ok && after_ok {
                out.push(i);
            }
            i += tb.len().max(1);
        } else {
            i += 1;
        }
    }
    out
}

/// Replace every whole-token occurrence; returns the new text and the number
/// of replacements.
pub fn replace_all(text: &str, old_token: &str, new_token: &str) -> (String, usize) {
    let positions = find_occurrences(text, old_token);
    if positions.is_empty() {
        return (text.to_string(), 0);
    }
    let mut out = String::with_capacity(text.len() + new_token.len() * positions.len());
    let mut last = 0usize;
    for &p in &positions {
        out.push_str(&text[last..p]);
        out.push_str(new_token);
        last = p + old_token.len();
    }
    out.push_str(&text[last..]);
    (out, positions.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_word_boundaries() {
        let text = "official style, nakano miku (school uniform), nakano mikuchan, nakano miku::";
        assert_eq!(count_occurrences(text, "nakano miku"), 2);
        let (replaced, n) = replace_all(text, "nakano miku", "hoshino ai");
        assert_eq!(n, 2);
        assert!(replaced.contains("hoshino ai (school uniform)"));
        assert!(replaced.contains("nakano mikuchan"));
        assert!(!replaced.contains("nakano miku::"));
    }

    #[test]
    fn multiword_token_with_parens_matches() {
        let text = ", official style, azki (4th costume) (hololive),, 4::completely nude::";
        let (out, n) = replace_all(text, "azki (4th costume) (hololive)", "elaina (majo no tabitabi)");
        assert_eq!(n, 1);
        assert!(out.contains("official style, elaina (majo no tabitabi),,"));
    }

    #[test]
    fn weight_prefix_token() {
        let text = "3::pantyhose, green skirt::  -2::pantyhose, green skirt::";
        let (out, n) = replace_all(text, "pantyhose", "black pantyhose");
        assert_eq!(n, 2);
        assert_eq!(out, "3::black pantyhose, green skirt::  -2::black pantyhose, green skirt::");
    }
}

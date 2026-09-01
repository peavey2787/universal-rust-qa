use std::collections::BTreeSet;

pub(super) fn calls(s: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    let mut cursor = 0;
    while s.as_bytes().get(cursor).is_some() {
        let Some((token, next_cursor)) = next_call_token(s, cursor) else {
            break;
        };
        set.insert(token);
        cursor = next_cursor.max(cursor.saturating_add(1));
    }
    set.into_iter().collect()
}

pub(super) fn next_call_token(s: &str, start: usize) -> Option<(String, usize)> {
    let mut cursor = start.min(s.len());
    while s.as_bytes().get(cursor).is_some() {
        let (token, end) = next_identifier(s, cursor)?;
        let next_cursor = end.max(cursor.saturating_add(1));
        if follows_call_paren(s, end) && !is_non_call_keyword(token) {
            return Some((token.trim_matches(':').to_string(), next_cursor));
        }
        cursor = next_cursor;
    }
    None
}

pub(super) fn next_identifier(s: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = s.as_bytes();
    let mut cursor = start.min(bytes.len());
    while bytes.get(cursor).is_some_and(|byte| !is_identifier_start(*byte)) {
        cursor = cursor.saturating_add(1);
    }
    bytes.get(cursor)?;
    let identifier_start = cursor;
    cursor = cursor.saturating_add(1);
    while bytes.get(cursor).is_some_and(|byte| is_identifier_continue(*byte)) {
        cursor = cursor.saturating_add(1);
    }
    Some((&s[identifier_start..cursor], cursor))
}

pub(super) fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

pub(super) fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b':'
}

pub(super) fn follows_call_paren(s: &str, cursor: usize) -> bool {
    s.as_bytes()
        .iter()
        .skip(cursor)
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'(')
}

pub(super) fn is_non_call_keyword(token: &str) -> bool {
    matches!(token, "if" | "for" | "while" | "match" | "loop" | "return" | "Some" | "Ok" | "Err")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_scanner_terminates_exactly_at_and_beyond_input_end() {
        assert_eq!(calls("alpha(); beta();"), vec!["alpha", "beta"]);
        let source = "alpha()";
        assert_eq!(next_call_token(source, source.len()), None);
        assert_eq!(next_call_token(source, source.len() + 10), None);
        assert_eq!(next_identifier(source, source.len()), None);
        assert_eq!(next_identifier("123", 0), None);
    }
}

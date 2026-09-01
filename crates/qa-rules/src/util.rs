#[derive(Clone, Copy)]
enum SanitizeState {
    Code,
    String,
    LineComment,
}

pub fn sanitize(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut state = SanitizeState::Code;
    while let Some(c) = chars.next() {
        state = sanitize_char(c, state, &mut chars, &mut out);
    }
    out
}

fn sanitize_char(
    c: char,
    state: SanitizeState,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    out: &mut String,
) -> SanitizeState {
    if matches!(state, SanitizeState::LineComment) {
        return sanitize_line_comment_char(c, out);
    }
    if matches!(state, SanitizeState::String) {
        return sanitize_string_char(c, chars, out);
    }
    sanitize_code_char(c, chars, out)
}

fn sanitize_code_char(
    c: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    out: &mut String,
) -> SanitizeState {
    if starts_line_comment(c, chars.peek().copied()) {
        out.push(' ');
        out.push(' ');
        let _ = chars.next();
        SanitizeState::LineComment
    } else if c == '"' {
        out.push(' ');
        SanitizeState::String
    } else {
        out.push(c);
        SanitizeState::Code
    }
}

fn starts_line_comment(c: char, next: Option<char>) -> bool {
    c == '/' && next == Some('/')
}

fn sanitize_string_char(
    c: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    out: &mut String,
) -> SanitizeState {
    if c == '\\' {
        out.push(' ');
        out.extend(chars.next().map(|_| ' '));
        SanitizeState::String
    } else if c == '"' {
        out.push(' ');
        SanitizeState::Code
    } else {
        out.push(masked_string_char(c));
        SanitizeState::String
    }
}

fn masked_string_char(c: char) -> char {
    if c == '\n' { '\n' } else { ' ' }
}

fn sanitize_line_comment_char(c: char, out: &mut String) -> SanitizeState {
    if c == '\n' {
        out.push(c);
        SanitizeState::Code
    } else {
        out.push(' ');
        SanitizeState::LineComment
    }
}

#[derive(Clone, Copy)]
enum CommentState {
    Code,
    String,
    LineComment,
    BlockComment(usize),
}

pub fn strip_comments_preserve_strings(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut state = CommentState::Code;
    while let Some(c) = chars.next() {
        state = match state {
            CommentState::Code => strip_code_char(c, &mut chars, &mut out),
            CommentState::String => strip_string_char(c, &mut chars, &mut out),
            CommentState::LineComment => strip_line_comment_char(c, &mut out),
            CommentState::BlockComment(depth) => {
                strip_block_comment_char(c, depth, &mut chars, &mut out)
            }
        };
    }
    out
}

fn strip_code_char(
    c: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    out: &mut String,
) -> CommentState {
    if c == '/' && chars.peek() == Some(&'/') {
        mask_pair(chars, out);
        CommentState::LineComment
    } else if c == '/' && chars.peek() == Some(&'*') {
        mask_pair(chars, out);
        CommentState::BlockComment(1)
    } else if c == '"' {
        out.push(c);
        CommentState::String
    } else {
        out.push(c);
        CommentState::Code
    }
}

fn strip_string_char(
    c: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    out: &mut String,
) -> CommentState {
    out.push(c);
    if c == '\\' {
        if let Some(next) = chars.next() {
            out.push(next);
        }
        CommentState::String
    } else if c == '"' {
        CommentState::Code
    } else {
        CommentState::String
    }
}

fn strip_line_comment_char(c: char, out: &mut String) -> CommentState {
    if c == '\n' {
        out.push(c);
        CommentState::Code
    } else {
        out.push(' ');
        CommentState::LineComment
    }
}

fn strip_block_comment_char(
    c: char,
    depth: usize,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    out: &mut String,
) -> CommentState {
    if c == '/' && chars.peek() == Some(&'*') {
        mask_pair(chars, out);
        CommentState::BlockComment(depth + 1)
    } else if c == '*' && chars.peek() == Some(&'/') {
        mask_pair(chars, out);
        if depth == 1 { CommentState::Code } else { CommentState::BlockComment(depth - 1) }
    } else {
        out.push(if c == '\n' { '\n' } else { ' ' });
        CommentState::BlockComment(depth)
    }
}

fn mask_pair(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    out.push(' ');
    out.push(' ');
    let _ = chars.next();
}

pub fn has_attr(a: &[String], n: &str) -> bool {
    a.iter().any(|x| x.contains(n))
}
pub fn policy_severity(p: &str) -> qa_model::Severity {
    if p.eq_ignore_ascii_case("deny") {
        qa_model::Severity::High
    } else {
        qa_model::Severity::Medium
    }
}

#[cfg(test)]
mod tests;

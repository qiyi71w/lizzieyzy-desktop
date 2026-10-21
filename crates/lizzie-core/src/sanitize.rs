//! Credential masking for persisted text, matching Java `PersistenceSanitizer`.
//!
//! Patterns run in source order, before each structural-escape canonicalization pass.
//! Percent decoding is only a bounded view of names and delimiters: decoded secret
//! bytes are never copied to the result. Exhausted scan/canonicalization bounds or
//! regex matching errors fail closed rather than returning partially sanitized text.

use std::borrow::Cow;
use std::sync::LazyLock;

use fancy_regex::Regex;

pub const FAILURE_MARKER: &str = "[redaction-failed]";
const REDACTED: &str = "<redacted>";
const MAX_CANONICALIZATION_PASSES: usize = 16;
const MAX_PERCENT_ENCODING_LAYERS: usize = 4;
const MAX_PERCENT_ENCODING_SCAN_LAYERS: usize = 64;
const MAX_CREDENTIAL_NAME_CHARS: usize = 64;
const CREDENTIAL_NAME: &str = r"(?:pw|password|passwd|token|secret|connectPassword|zhizi-account-token|zz-socketio-token|machine[-_]?key|(?:x[-_]?)?api[-_]?key|(?:x[-_]?)?access[-_]?token|refresh[-_]?token|client[-_]?secret)";
const HEADER_NAME: &str = r"(?:proxy-authorization|authorization|set-cookie|cookie)";

struct Rule {
    pattern: Regex,
    replacement: &'static str,
}

impl Rule {
    fn new(pattern: &str, replacement: &'static str) -> Self {
        Self {
            pattern: compile(pattern),
            replacement,
        }
    }

    fn apply(&self, safe: &mut Cow<'_, str>) -> Result<(), ()> {
        // Unlike replace_all, try_replacen reports backtracking-limit failures.
        // Capture expansion preserves literal replacement metacharacters in input.
        match self.pattern.try_replacen(safe.as_ref(), 0, self.replacement) {
            Ok(Cow::Owned(replaced)) => *safe = Cow::Owned(replaced),
            Ok(Cow::Borrowed(_)) => {}
            Err(_) => return Err(()),
        }
        Ok(())
    }
}

struct Patterns {
    command: Rule,
    remaining: [Rule; 9],
    credential_name: Regex,
    header_name: Regex,
}

static PATTERNS: LazyLock<Patterns> = LazyLock::new(|| Patterns {
    command: Rule::new(
        r#"(?i)((?<![A-Za-z0-9_-])["']?(?:--?|/){NAME}["']?\s+)(?>(?:"(?>(?:\\[\s\S])|[^"\\])*(?:"|$))|(?:'(?>(?:\\[\s\S])|[^'\\])*(?:'|$))|\\[\s\S]|[^ "'\\])+"#,
        "${1}<redacted>",
    ),
    remaining: [
        Rule::new(
            r#"(?i)(?<![A-Za-z0-9_-])(["']?{HEADER}["']?\s*(?:[:=]\s*|\s+))[^\r\n]*"#,
            "${1}<redacted>",
        ),
        Rule::new(
            r#"(?i)(?<![A-Za-z0-9_-])(["'])((?:--?|/)?{NAME}\s*=\s*)(?>(?:\\[\s\S])|(?!\1)[^,\r\n])*(\1|,|$)"#,
            "${1}${2}<redacted>${3}",
        ),
        Rule::new(
            r#"(?i)(?:(?<![A-Za-z0-9_-])((?:--?|/)?{NAME}\s*=\s*)(?>(?:\\[\s\S])|[^"',\r\n])+(?=,)|(?<=,)((?:--?|/)?{NAME}\s*=\s*)(?>(?:\\[\s\S])|[^"',\r\n])+(?=[,"']|$))"#,
            "${1}${2}<redacted>",
        ),
        Rule::new(
            r#"(?i)((?<![A-Za-z0-9_-])(?:--?|/)?["']?{NAME}["']?(?![A-Za-z0-9_-])\s*(?:[=:]\s*|\s+))(["'])(?>(?:\\[\s\S])|(?!\2)[\s\S])*(?:\2|$)"#,
            "${1}${2}<redacted>${2}",
        ),
        Rule::new(
            r#"(?i)((?<![A-Za-z0-9_-])(?:--?|/)?["']?{NAME}["']?(?![A-Za-z0-9_-])\s*(?:[=:]\s*|\s+))([^"'\s,;}&]+)"#,
            "${1}<redacted>",
        ),
        Rule::new(
            r"(?i)([?&](?:token|key|secret|password|(?:x[-_]?)?api[-_]?key|(?:x[-_]?)?access[-_]?token|refresh[-_]?token|client[-_]?secret)=)[^&\s]+",
            "${1}<redacted>",
        ),
        Rule::new(r"(?i)(\b[a-z][a-z0-9+.-]*://)[^/@\s]+@", "${1}<redacted>@"),
        Rule::new(r#"(?i)(?<![A-Za-z0-9_-])(bearer\s+)[^\s,;"'<>}\]]+"#, "${1}<redacted>"),
        Rule::new(r"(?i)(?<![A-Za-z0-9_-])(basic\s+)[A-Za-z0-9+/=_-]+", "${1}<redacted>"),
    ],
    credential_name: compile(r"(?i)^{NAME}$"),
    header_name: compile(r"(?i)^{HEADER}$"),
});

fn compile(pattern: &str) -> Regex {
    let pattern = pattern
        .replace("{NAME}", CREDENTIAL_NAME)
        .replace("{HEADER}", HEADER_NAME)
        .replace(r"[\s\S]", "(?s:.)");
    // Java Pattern's default \s is ASCII, whereas Rust's default is Unicode.
    // Expand it differently inside/outside character classes, without changing
    // the any-character [\s\S] alternatives or the patterns' capture numbers.
    let mut expanded = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    let mut in_class = false;
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('s') if in_class => expanded.push_str(r"\x20\t\n\x0B\f\r"),
                Some('s') => expanded.push_str(r"[\x20\t\n\x0B\f\r]"),
                Some(escaped) => {
                    expanded.push('\\');
                    expanded.push(escaped);
                }
                None => expanded.push('\\'),
            }
        } else {
            if ch == '[' {
                in_class = true;
            } else if ch == ']' {
                in_class = false;
            }
            expanded.push(ch);
        }
    }
    Regex::new(&expanded).expect("static Java credential pattern must compile")
}

/// Masks credentials before text is persisted, retaining non-secret context.
///
/// Empty input remains empty. Encoded structural syntax is canonicalized only
/// after masking; unsafe scan depth or regex evaluation returns [`FAILURE_MARKER`].
pub fn sanitize(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut safe = Cow::Borrowed(text);
    for _ in 0..MAX_CANONICALIZATION_PASSES {
        if sanitize_canonical(&mut safe).is_err() {
            return FAILURE_MARKER.to_owned();
        }
        if !contains_encoded_syntax(&safe) {
            return safe.into_owned();
        }
        let canonical = canonicalize_encoded_syntax(&safe);
        if canonical == safe {
            return safe.into_owned();
        }
        safe = Cow::Owned(canonical);
    }
    FAILURE_MARKER.to_owned()
}

fn sanitize_canonical(safe: &mut Cow<'_, str>) -> Result<(), ()> {
    PATTERNS.command.apply(safe)?;
    if let Some(redacted) = redact_percent_encoded_credentials(safe)? {
        *safe = Cow::Owned(redacted);
    }
    for rule in &PATTERNS.remaining {
        rule.apply(safe)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct DecodedCharacter {
    value: char,
    end: usize,
    encoded: bool,
    // Java records this threshold but still scans/masks deeper prefixes until
    // the absolute scan bound. Exceeding four layers alone must not fail open.
    _over_depth: bool,
}

const END: DecodedCharacter = DecodedCharacter {
    value: '\0',
    end: 0,
    encoded: false,
    _over_depth: false,
};

fn decoded_character_at(text: &str, index: usize) -> Result<DecodedCharacter, ()> {
    let Some(value) = text.get(index..).and_then(|rest| rest.chars().next()) else {
        return Ok(END);
    };
    if value != '%' {
        return Ok(DecodedCharacter {
            value,
            end: index + value.len_utf8(),
            ..END
        });
    }
    let mut digits = index + 1;
    let mut layers = 1;
    while digits + 3 < text.len() && hex_byte(text, digits) == Some(b'%') {
        if layers >= MAX_PERCENT_ENCODING_SCAN_LAYERS {
            return Err(());
        }
        digits += 2;
        layers += 1;
    }
    match hex_byte(text, digits) {
        Some(decoded) if decoded.is_ascii() => Ok(DecodedCharacter {
            value: char::from(decoded),
            end: digits + 2,
            encoded: true,
            _over_depth: layers > MAX_PERCENT_ENCODING_LAYERS,
        }),
        _ => Ok(DecodedCharacter {
            value: '%',
            end: index + 1,
            ..END
        }),
    }
}

fn hex_byte(text: &str, digits: usize) -> Option<u8> {
    let bytes = text.as_bytes().get(digits..digits.checked_add(2)?)?;
    let high = char::from(bytes[0]).to_digit(16)?;
    let low = char::from(bytes[1]).to_digit(16)?;
    Some(((high << 4) | low) as u8)
}

fn is_name_character(value: char) -> bool {
    value.is_ascii_alphanumeric() || value == '_' || value == '-'
}

// Character.isWhitespace excludes the nonbreaking Unicode spaces that Rust's
// char::is_whitespace includes, and includes the Java information separators.
fn java_whitespace(value: char) -> bool {
    matches!(value, '\u{0009}'..='\u{000d}' | '\u{001c}'..='\u{0020}' | '\u{1680}' | '\u{2000}'..='\u{2006}' | '\u{2008}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{205f}' | '\u{3000}')
}

fn credential_boundary_at(text: &str, start: usize) -> Option<bool> {
    let Some(previous) = text[..start].chars().next_back() else {
        return Some(false);
    };
    if !is_name_character(previous) {
        return Some(false);
    }
    for layers in 1..=MAX_PERCENT_ENCODING_SCAN_LAYERS {
        let Some(encoded_start) = start.checked_sub(1 + layers * 2) else {
            break;
        };
        // Java ignores a limit-exceeded view while looking backward for a
        // boundary. Forward prefix/value scanning owns fail-closed decisions.
        let decoded = decoded_character_at(text, encoded_start).unwrap_or(END);
        if decoded.encoded && decoded.end == start {
            return (!is_name_character(decoded.value)).then_some(true);
        }
    }
    None
}

struct PercentCredentialPrefix {
    end: usize,
    header: bool,
}

fn percent_credential_prefix_at(text: &str, start: usize) -> Result<Option<PercentCredentialPrefix>, ()> {
    let Some(mut encoded) = credential_boundary_at(text, start) else {
        return Ok(None);
    };
    let mut index = start;
    let mut next = decoded_character_at(text, index)?;
    if matches!(next.value, '"' | '\'') {
        encoded |= next.encoded;
        index = next.end;
    }
    // Credential names are bounded ASCII; no heap allocation per scan position.
    let mut name = [0_u8; MAX_CREDENTIAL_NAME_CHARS];
    let mut name_len = 0;
    while index < text.len() && name_len < MAX_CREDENTIAL_NAME_CHARS {
        next = decoded_character_at(text, index)?;
        if !is_name_character(next.value) {
            break;
        }
        name[name_len] = next.value as u8;
        name_len += 1;
        encoded |= next.encoded;
        index = next.end;
    }
    if name_len == 0 {
        return Ok(None);
    }
    next = decoded_character_at(text, index)?;
    if is_name_character(next.value) {
        return Ok(None);
    }
    if matches!(next.value, '"' | '\'') {
        encoded |= next.encoded;
        index = next.end;
    }
    let name = std::str::from_utf8(&name[..name_len]).expect("credential names are ASCII");
    let header = PATTERNS.header_name.is_match(name).map_err(|_| ())?;
    let credential = PATTERNS.credential_name.is_match(name).map_err(|_| ())?;
    if !header && !credential {
        return Ok(None);
    }
    let mut had_whitespace = false;
    while index < text.len() {
        next = decoded_character_at(text, index)?;
        if !java_whitespace(next.value) {
            break;
        }
        had_whitespace = true;
        encoded |= next.encoded;
        index = next.end;
    }
    next = decoded_character_at(text, index)?;
    if matches!(next.value, '=' | ':') {
        encoded |= next.encoded;
        index = next.end;
        while index < text.len() {
            next = decoded_character_at(text, index)?;
            if !java_whitespace(next.value) {
                break;
            }
            encoded |= next.encoded;
            index = next.end;
        }
    } else if !had_whitespace {
        return Ok(None);
    }
    Ok(encoded.then_some(PercentCredentialPrefix { end: index, header }))
}

fn is_unquoted_delimiter(value: char) -> bool {
    matches!(value, '"' | '\'' | ',' | ';' | '}' | '&') || java_whitespace(value)
}

fn credential_value_end(text: &str, prefix: &PercentCredentialPrefix) -> Option<usize> {
    let start = prefix.end;
    let first = text.get(start..)?.chars().next()?;
    if matches!(first, '\r' | '\n') {
        return None;
    }
    if prefix.header {
        return Some(text[start..].find(['\r', '\n']).map_or(text.len(), |end| start + end));
    }
    if matches!(first, '"' | '\'') {
        let mut chars = text[start + 1..].char_indices();
        while let Some((offset, current)) = chars.next() {
            if current == '\\' {
                chars.next();
            } else if current == first {
                return Some(start + 1 + offset + 1);
            }
        }
        return Some(text.len());
    }
    if is_unquoted_delimiter(first) {
        return None;
    }
    Some(
        text[start..]
            .find(is_unquoted_delimiter)
            .map_or(text.len(), |end| start + end),
    )
}

fn redact_percent_encoded_credentials(text: &str) -> Result<Option<String>, ()> {
    if !text.contains('%') {
        return Ok(None);
    }
    let mut redacted: Option<String> = None;
    let mut copied_through = 0;
    let mut index = 0;
    while index < text.len() {
        if text.as_bytes()[index] == b'%' {
            decoded_character_at(text, index)?;
        }
        let Some(prefix) = percent_credential_prefix_at(text, index)? else {
            index += text[index..].chars().next().expect("index in text").len_utf8();
            continue;
        };
        let Some(value_end) = credential_value_end(text, &prefix) else {
            index += text[index..].chars().next().expect("index in text").len_utf8();
            continue;
        };
        let output = redacted.get_or_insert_with(|| String::with_capacity(text.len()));
        output.push_str(&text[copied_through..prefix.end]);
        let first = text.as_bytes()[prefix.end];
        if !prefix.header && matches!(first, b'"' | b'\'') {
            output.push(char::from(first));
            output.push_str(REDACTED);
            output.push(char::from(first));
        } else {
            output.push_str(REDACTED);
        }
        copied_through = value_end;
        index = value_end;
    }
    if let Some(output) = &mut redacted {
        output.push_str(&text[copied_through..]);
    }
    Ok(redacted)
}

fn contains_encoded_syntax(text: &str) -> bool {
    text.as_bytes()
        .windows(2)
        .any(|pair| pair[0] == b'\\' && matches!(pair[1], b'"' | b'\'' | b'u' | b'U'))
}

fn canonicalize_encoded_syntax(text: &str) -> String {
    let mut canonical = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let current = text[index..].chars().next().expect("index in text");
        if current != '\\' {
            canonical.push(current);
            index += current.len_utf8();
            continue;
        }
        let mut escape_end = index;
        while text.as_bytes().get(escape_end) == Some(&b'\\') {
            escape_end += 1;
        }
        let slash_count = escape_end - index;
        for _ in 0..slash_count / 2 {
            canonical.push('\\');
        }
        if slash_count % 2 == 0 {
            index = escape_end;
            continue;
        }
        match text.as_bytes().get(escape_end) {
            Some(b'"' | b'\'') => {
                canonical.push(char::from(text.as_bytes()[escape_end]));
                index = escape_end + 1;
                continue;
            }
            Some(b'u' | b'U') => {
                if let Some(decoded) = decode_structural_escape(text, escape_end + 1) {
                    canonical.push(decoded);
                    index = escape_end + 5;
                    continue;
                }
            }
            _ => {}
        }
        canonical.push('\\');
        index = escape_end;
    }
    canonical
}

fn decode_structural_escape(text: &str, digits: usize) -> Option<char> {
    let bytes = text.as_bytes().get(digits..digits.checked_add(4)?)?;
    let mut value = 0;
    for byte in bytes {
        value = value * 16 + char::from(*byte).to_digit(16)?;
    }
    match value {
        0x0009 | 0x000a | 0x000d | 0x0020 => Some(' '),
        0x0022 => Some('"'),
        0x0027 => Some('\''),
        0x003a => Some(':'),
        0x003d => Some('='),
        0x005c => Some('\\'),
        _ if value <= 0x7f => {
            let decoded = char::from(value as u8);
            is_name_character(decoded).then_some(decoded)
        }
        _ => None,
    }
}

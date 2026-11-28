//! Credential masking for persisted text, matching Java `PersistenceSanitizer`.
//!
//! Patterns run in source order, before each structural-escape canonicalization pass.
//! Percent decoding is only a bounded view of names and delimiters: decoded secret
//! bytes are never copied to the result. Exhausted scan/canonicalization bounds or
//! regex matching errors fail closed rather than returning partially sanitized text.
//!
//! Unlike Java, canonicalization only rewrites the result when it exposes a credential
//! the text as written hid behind escapes. Otherwise the first pass's result is kept,
//! so `\\server\share`, `\u0041` and `\"` stay as typed instead of being decoded.
//!
//! Also unlike Java, a value written between escaped quotes (`password=\"a b\"`,
//! `-pw \"a b\"`) is masked up to the closing escaped quote, and `\u0025` decodes to
//! `%` so a percent-encoded assignment hidden behind it is found. Java masked only
//! the text before the first escape or space and left the rest of the value visible.

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
// A value between escaped quotes, read the way canonicalization would decode it: a run of n
// backslashes before a quote decodes to n/2 backslashes and a quote, so the value ends where
// n is 1, 5, 9, … (the decoded quote is not itself escaped) and goes on through `\\\"` (an
// escaped quote inside it) and an unescaped quote. It never ends before the quote
// canonicalization ends it at, and not after it for text written with one level of escapes.
const ESCAPED_DOUBLE_CHARS: &str = r#"(?>\\\\\\\\|\\\\\\["']|\\\\|(?!\\")[\s\S])*"#;
const ESCAPED_SINGLE_CHARS: &str = r#"(?>\\\\\\\\|\\\\\\["']|\\\\|(?!\\')[\s\S])*"#;

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
    remaining: [Rule; 10],
    escaped_value: Regex,
    credential_name: Regex,
    header_name: Regex,
}

static PATTERNS: LazyLock<Patterns> = LazyLock::new(|| Patterns {
    command: Rule::new(
        r#"(?i)((?<![A-Za-z0-9_-])["']?(?:--?|/){NAME}["']?\s+)(?>(?:"(?>(?:\\[\s\S])|[^"\\])*(?:"|$))|(?:'(?>(?:\\[\s\S])|[^'\\])*(?:'|$))|{ESCAPED_QUOTED}|\\[\s\S]|[^ "'\\])+"#,
        "${1}<redacted>",
    ),
    remaining: [
        Rule::new(
            r#"(?i)(?<![A-Za-z0-9_-])(["']?{HEADER}["']?\s*(?:[:=]\s*|\s+))[^\r\n]*"#,
            "${1}<redacted>",
        ),
        // A whole assignment inside quotes. A value between escaped quotes that close before the
        // outer quote is left to the next rule, which keeps a comma inside them; one that does
        // not close there runs to the outer quote. An empty value is one only where the outer
        // quote ends the word: in `"password="<redacted>"` that quote opens the masked value.
        Rule::new(
            r#"(?i)(?<![A-Za-z0-9_-])(["'])((?:--?|/)?{NAME}(?>\s*=\s*))(?!\\"(?>\\\\\\\\|\\\\\\["']|\\\\|(?!\\"|\1)[\s\S])*\\"|\\'(?>\\\\\\\\|\\\\\\["']|\\\\|(?!\\'|\1)[\s\S])*\\')(?:\\["'](?>(?:\\[\s\S])|(?!\1)[^\r\n])*|(?>(?:\\[\s\S])|(?!\1)[^,\r\n])+|(?=\1(?:[\s,]|$)))(\1|,|$)"#,
            "${1}${2}<redacted>${3}",
        ),
        // Before the rules that read `\"` as one escaped character of a longer value: they would
        // end it at a comma or a space inside the quotes. They leave values opened by `\"` or
        // `\'` to this one, also once it has masked them.
        Rule::new(
            r#"(?i)((?<![A-Za-z0-9_-])(?:--?|/)?["']?{NAME}["']?(?![A-Za-z0-9_-])\s*(?:[=:]\s*|\s+))(?:(\\"){ESCAPED_DOUBLE}|(\\'){ESCAPED_SINGLE})"#,
            "${1}${2}${3}<redacted>${2}${3}",
        ),
        // The assignment prefix is atomic: giving back a space after `=` would let the value
        // start before an escaped quote the rule above already masked.
        Rule::new(
            r#"(?i)(?:(?<![A-Za-z0-9_-])((?:--?|/)?{NAME}(?>\s*=\s*))(?!\\["'])(?>(?:\\[\s\S])|[^"',\r\n])+(?=,)|(?<=,)((?:--?|/)?{NAME}(?>\s*=\s*))(?!\\["'])(?>(?:\\[\s\S])|[^"',\r\n])+(?=[,"']|$))"#,
            "${1}${2}<redacted>",
        ),
        Rule::new(
            r#"(?i)((?<![A-Za-z0-9_-])(?:--?|/)?["']?{NAME}["']?(?![A-Za-z0-9_-])\s*(?:[=:]\s*|\s+))(["'])(?>(?:\\[\s\S])|(?!\2)[\s\S])*(?:\2|$)"#,
            "${1}${2}<redacted>${2}",
        ),
        Rule::new(
            r#"(?i)((?<![A-Za-z0-9_-])(?:--?|/)?["']?{NAME}["']?(?![A-Za-z0-9_-])\s*(?:[=:]\s*|\s+))(?!\\["'])([^"'\s,;}&]+)"#,
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
    escaped_value: compile(r"^{ESCAPED_QUOTED}"),
    credential_name: compile(r"(?i)^{NAME}$"),
    header_name: compile(r"(?i)^{HEADER}$"),
});

fn compile(pattern: &str) -> Regex {
    let pattern = pattern
        .replace("{ESCAPED_QUOTED}", r#"(?:\\"{ESCAPED_DOUBLE}|\\'{ESCAPED_SINGLE})"#)
        .replace("{ESCAPED_DOUBLE}", r#"{ESCAPED_DOUBLE_CHARS}(?:\\"|$)"#)
        .replace("{ESCAPED_SINGLE}", r#"{ESCAPED_SINGLE_CHARS}(?:\\'|$)"#)
        .replace("{ESCAPED_DOUBLE_CHARS}", ESCAPED_DOUBLE_CHARS)
        .replace("{ESCAPED_SINGLE_CHARS}", ESCAPED_SINGLE_CHARS)
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

/// Masks credentials before text is persisted, retaining non-secret context as written.
///
/// Empty input remains empty. Encoded structural syntax is canonicalized only
/// after masking, and the canonical form is returned only when masking it found a
/// credential the written form hid; unsafe scan depth or regex evaluation returns
/// [`FAILURE_MARKER`].
pub fn sanitize(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut written = Cow::Borrowed(text);
    if sanitize_canonical(&mut written).is_err() {
        return FAILURE_MARKER.to_owned();
    }
    let mut safe = Cow::Borrowed(written.as_ref());
    let mut hidden_credential = false;
    let mut passes = 1;
    while contains_encoded_syntax(&safe) {
        let canonical = canonicalize_encoded_syntax(&safe);
        if canonical == safe {
            break;
        }
        if passes == MAX_CANONICALIZATION_PASSES {
            return FAILURE_MARKER.to_owned();
        }
        passes += 1;
        let mut masked = Cow::Borrowed(canonical.as_str());
        if sanitize_canonical(&mut masked).is_err() {
            return FAILURE_MARKER.to_owned();
        }
        safe = match masked {
            Cow::Owned(masked) => {
                hidden_credential |= masked != canonical;
                Cow::Owned(masked)
            }
            Cow::Borrowed(_) => Cow::Owned(canonical),
        };
    }
    // Without a hidden credential the canonical text holds the same secrets as the
    // written one, so the written form, already masked, is as safe.
    if hidden_credential {
        safe.into_owned()
    } else {
        written.into_owned()
    }
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

/// Where the value after `prefix` ends, and the quote around it (`"`, `'`, `\"`, `\'` or none)
/// that the masked value keeps.
fn credential_value_end<'a>(text: &'a str, prefix: &PercentCredentialPrefix) -> Result<Option<(usize, &'a str)>, ()> {
    let start = prefix.end;
    let Some(first) = text.get(start..).and_then(|rest| rest.chars().next()) else {
        return Ok(None);
    };
    if matches!(first, '\r' | '\n') {
        return Ok(None);
    }
    if prefix.header {
        return Ok(Some((text[start..].find(['\r', '\n']).map_or(text.len(), |end| start + end), "")));
    }
    if matches!(first, '"' | '\'') {
        let mut chars = text[start + 1..].char_indices();
        while let Some((offset, current)) = chars.next() {
            if current == '\\' {
                chars.next();
            } else if current == first {
                return Ok(Some((start + 1 + offset + 1, &text[start..start + 1])));
            }
        }
        return Ok(Some((text.len(), &text[start..start + 1])));
    }
    if let Some(escaped) = PATTERNS.escaped_value.find(&text[start..]).map_err(|_| ())? {
        return Ok(Some((start + escaped.end(), &text[start..start + 2])));
    }
    if is_unquoted_delimiter(first) {
        return Ok(None);
    }
    Ok(Some((
        text[start..]
            .find(is_unquoted_delimiter)
            .map_or(text.len(), |end| start + end),
        "",
    )))
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
        let Some((value_end, quote)) = credential_value_end(text, &prefix)? else {
            index += text[index..].chars().next().expect("index in text").len_utf8();
            continue;
        };
        let output = redacted.get_or_insert_with(|| String::with_capacity(text.len()));
        output.push_str(&text[copied_through..prefix.end]);
        output.push_str(quote);
        output.push_str(REDACTED);
        output.push_str(quote);
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
        // Lets `\u0025` reveal a percent-encoded name or delimiter to the percent scan.
        0x0025 => Some('%'),
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

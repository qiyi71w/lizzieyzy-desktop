use super::{java_trim, CommentaryFailure};
pub const BEGIN: &str = "[LizzieYzy AI Commentary BEGIN]";
pub const END: &str = "[LizzieYzy AI Commentary END]";
fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}
fn escape(s: &str) -> String {
    normalize(s)
        .replace(BEGIN, "[LizzieYzy AI Commentary BEGIN escaped]")
        .replace(END, "[LizzieYzy AI Commentary END escaped]")
}
pub fn upsert(original: &str, commentary: &str, model: &str, timestamp: &str) -> Result<String, CommentaryFailure> {
    let escaped = escape(commentary);
    let body = java_trim(&escaped);
    if body.is_empty() {
        return Err(CommentaryFailure::new("AI commentary is empty."));
    }
    if body.encode_utf16().count() > 100_000 {
        return Err(CommentaryFailure::new("AI commentary is too large for an SGF comment."));
    }
    let original = remove_blocks(original);
    let model = escape(model).replace('\n', " ");
    let model = java_trim(&model);
    let mut block = format!("{BEGIN}\ngeneratedAt={timestamp}\n");
    if !model.is_empty() {
        block.push_str(&format!("model={model}\n"));
    }
    block.push_str(&format!("\n{body}\n{END}"));
    Ok(if original.is_empty() {
        block
    } else {
        format!("{original}\n\n{block}")
    })
}
pub fn extract(comment: &str) -> Option<String> {
    let source = normalize(comment);
    let mut search = source.len();
    while search > 0 {
        let start = source[..search].rfind(BEGIN)?;
        if let Some(end) = source[start + BEGIN.len()..].find(END) {
            let block = java_trim(&source[start + BEGIN.len()..start + BEGIN.len() + end]);
            let body = java_trim(block.split_once("\n\n").map_or(block, |(_, b)| b));
            return (!body.is_empty()).then(|| body.into());
        }
        search = start;
    }
    None
}
pub fn remove_blocks(comment: &str) -> String {
    let mut result = comment.to_owned();
    let mut search = result.len();
    while search > 0 {
        let Some(start) = result[..search].rfind(BEGIN) else {
            break;
        };
        let Some(end) = result[start + BEGIN.len()..].find(END) else {
            search = start;
            continue;
        };
        let after = start + BEGIN.len() + end + END.len();
        let from = if result[..start].ends_with("\n\n") {
            start - 2
        } else {
            start
        };
        result.replace_range(from..after, "");
        search = from;
    }
    result
}

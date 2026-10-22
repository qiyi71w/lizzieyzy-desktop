//! 导出为xls of the 超级鹰眼 lists (`Utils.exportTable`): not a spreadsheet file but the cells as
//! tab-separated text, UTF-16LE with a byte-order mark (`x-UTF-16LE-BOM`), which spreadsheet
//! programs open under the `.xls` name. Every cell, the last of a row included, is followed by a
//! tab; each row ends with the platform line separator (`BufferedWriter.newLine`).

use std::path::Path;

/// The file Java writes for the chosen `path` (`MoveListFrame` export listener): `.xls` is always
/// appended to the chosen name; a name containing `.xlsx` writes nothing.
pub fn xls_target(path: &str) -> Option<String> {
    let name = Path::new(path).file_name()?.to_string_lossy();
    (!name.contains(".xlsx")).then(|| format!("{path}.xls"))
}

/// The bytes of `rows` (the header first) with `newline` after each row.
pub fn encode(rows: &[Vec<String>], newline: &str) -> Vec<u8> {
    let mut text = String::new();
    for row in rows {
        for cell in row {
            text.push_str(cell);
            text.push('\t');
        }
        text.push_str(newline);
    }
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// The platform line separator Java's `newLine` writes.
pub const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

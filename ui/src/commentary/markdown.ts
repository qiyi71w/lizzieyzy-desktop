// SafeMarkdownRenderer.java: the small, HTML-free Markdown subset of AI commentary. Every character
// taken from the text is escaped (`& < > " '`), code included; there are no links, images or raw
// HTML, so the result can be put into the DOM as markup. Java string semantics are kept: `trim`
// strips code points up to U+0020, `\s` is ASCII whitespace and `.` stops at line terminators.

/** String.trim. */
function javaTrim(s: string): string {
  let start = 0;
  let end = s.length;
  while (start < end && s.charCodeAt(start) <= 0x20) start++;
  while (end > start && s.charCodeAt(end - 1) <= 0x20) end--;
  return s.slice(start, end);
}

/** Character.isWhitespace for one UTF-16 unit. */
function javaWhitespace(c: string): boolean {
  const code = c.charCodeAt(0);
  if ((code >= 0x09 && code <= 0x0d) || (code >= 0x1c && code <= 0x20)) return true;
  if (code === 0xa0 || code === 0x2007 || code === 0x202f) return false;
  return /[\u1680\u2000-\u2006\u2008-\u200a\u2028\u2029\u205f\u3000]/.test(c);
}

/** `line.matches("\\d+\\.\\s+.*")`. */
const ORDERED = /^\d+\.[ \t\n\x0B\f\r]+[^\n\r\u0085\u2028\u2029]*$/;
/** `cell.trim().matches(":?-{3,}:?")`. */
const SEPARATOR = /^:?-{3,}:?$/;

const ESCAPES: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

export function escape(text: string): string {
  return text.replace(/[&<>"']/g, (c) => ESCAPES[c]);
}

function tableCells(line: string): string[] {
  if (line.indexOf("|") < 0) return [];
  const raw = javaTrim(line).split("|");
  const start = raw.length > 0 && javaTrim(raw[0]) === "" ? 1 : 0;
  const end = raw.length > start && javaTrim(raw[raw.length - 1]) === "" ? raw.length - 1 : raw.length;
  return end <= start ? [] : raw.slice(start, end);
}

function isTableSeparator(line: string, columns: number): boolean {
  const cells = tableCells(line);
  return cells.length === columns && cells.every((cell) => SEPARATOR.test(javaTrim(cell)));
}

function inline(text: string): string {
  let html = "";
  let index = 0;
  while (index < text.length) {
    if (text[index] === "`") {
      const end = text.indexOf("`", index + 1);
      if (end > index + 1) {
        html += `<code>${escape(text.slice(index + 1, end))}</code>`;
        index = end + 1;
        continue;
      }
    }
    if (text.startsWith("**", index)) {
      const end = text.indexOf("**", index + 2);
      if (end > index + 2) {
        html += `<strong>${escape(text.slice(index + 2, end))}</strong>`;
        index = end + 2;
        continue;
      }
    }
    if (text[index] === "*") {
      const end = text.indexOf("*", index + 1);
      if (end > index + 1) {
        html += `<em>${escape(text.slice(index + 1, end))}</em>`;
        index = end + 1;
        continue;
      }
    }
    html += escape(text[index]);
    index++;
  }
  return html;
}

/** SafeMarkdownRenderer.toBodyHtml. */
export function markdownToHtml(markdown: string): string {
  if (!markdown) return "";
  const lines = markdown.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  let html = "";
  let openList: string | null = null;
  const closeList = () => {
    if (openList) html += `</${openList}>`;
    openList = null;
  };
  let inCode = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.startsWith("```")) {
      closeList();
      html += inCode ? "</code></pre>" : "<pre><code>";
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      html += `${escape(line)}\n`;
      continue;
    }
    const header = tableCells(line);
    if (header.length && i + 1 < lines.length && isTableSeparator(lines[i + 1], header.length)) {
      closeList();
      html += "<table><thead><tr>";
      for (const cell of header) html += `<th>${inline(javaTrim(cell))}</th>`;
      html += "</tr></thead><tbody>";
      i++;
      while (i + 1 < lines.length) {
        const row = tableCells(lines[i + 1]);
        if (row.length !== header.length) break;
        i++;
        html += "<tr>";
        for (const cell of row) html += `<td>${inline(javaTrim(cell))}</td>`;
        html += "</tr>";
      }
      html += "</tbody></table>";
      continue;
    }
    const unordered = line.startsWith("- ") || line.startsWith("* ");
    const ordered = ORDERED.test(line);
    if (unordered || ordered) {
      const type = unordered ? "ul" : "ol";
      if (type !== openList) {
        closeList();
        html += `<${type}>`;
        openList = type;
      }
      let start = unordered ? 2 : line.indexOf(".") + 1;
      while (start < line.length && javaWhitespace(line[start])) start++;
      html += `<li>${inline(line.slice(start))}</li>`;
      continue;
    }
    closeList();
    if (line.startsWith("### ")) html += `<h3>${inline(line.slice(4))}</h3>`;
    else if (line.startsWith("## ")) html += `<h2>${inline(line.slice(3))}</h2>`;
    else if (line.startsWith("# ")) html += `<h1>${inline(line.slice(2))}</h1>`;
    else if (line.startsWith("> ")) html += `<blockquote>${inline(line.slice(2))}</blockquote>`;
    else if (javaTrim(line) === "") html += "<div>&nbsp;</div>";
    else html += `<div>${inline(line)}</div>`;
  }
  closeList();
  if (inCode) html += "</code></pre>";
  return html;
}

/** SafeMarkdownRenderer.plainTextToBodyHtml: one line per block, repeated spaces and tabs kept. */
export function plainTextToHtml(text: string): string {
  if (!text) return "";
  let html = "";
  for (const line of text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n")) {
    html += "<div>";
    if (!line) html += "&nbsp;";
    else {
      let spaces = 0;
      for (const c of line) {
        if (c === " ") {
          spaces++;
          html += spaces === 1 ? " " : "&nbsp;";
        } else if (c === "\t") {
          spaces = 0;
          html += "&nbsp;&nbsp;&nbsp;&nbsp;";
        } else {
          spaces = 0;
          html += escape(c);
        }
      }
    }
    html += "</div>";
  }
  return html;
}

#!/usr/bin/env python3
"""Regenerate ui/src/i18n/javaStrings.json from the fixed Java resource bundles.

The UI shows Java's own wording. This script collects every resource key the UI uses (`t("Key")`
calls and `"key"` entries in ui/src/chrome/javaMenus.ts) and copies those entries from the Java
DisplayStrings bundles, per locale, falling back to the base bundle like Java's ResourceBundle.

Usage: tools/extract_java_strings.py <java-checkout>
The Java checkout must be wimi321/lizzieyzy-next at cb53acc55eda42c6c671b5f668cd3b597456bf07.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXED_SHA = "cb53acc55eda42c6c671b5f668cd3b597456bf07"
LOCALES = {
    "ZhCn": "DisplayStrings_zh_CN.properties",
    "EnUs": "DisplayStrings_en_US.properties",
    "ZhTw": "DisplayStrings_zh_TW.properties",
    "JaJp": "DisplayStrings_ja_JP.properties",
    "KoKr": "DisplayStrings_ko.properties",
    "ThTh": "DisplayStrings_th_TH.properties",
}


def load_properties(path: Path) -> dict[str, str]:
    """java.util.Properties text format: continuation lines, escapes, \\uXXXX."""
    lines = path.read_text(encoding="utf-8").replace("\r\n", "\n").split("\n")
    logical: list[str] = []
    buf = ""
    for line in lines:
        if buf:
            line = buf + line.lstrip()
            buf = ""
        trailing = len(line) - len(line.rstrip("\\"))
        if trailing % 2 == 1:
            buf = line[:-1]
            continue
        logical.append(line)
    out: dict[str, str] = {}
    for line in logical:
        s = line.lstrip()
        if not s or s[0] in "#!":
            continue
        m = re.match(r"((?:[^=:\s\\]|\\.)+)\s*[=:]?\s*(.*)$", s)
        if not m:
            continue
        out[unescape(m.group(1))] = unescape(m.group(2))
    return out


def unescape(s: str) -> str:
    def rep(m: re.Match) -> str:
        c = m.group(1)
        if c.startswith("u"):
            return chr(int(c[1:], 16))
        return {"n": "\n", "t": "\t", "r": "\r", "f": "\f"}.get(c, c)

    return re.sub(r"\\(u[0-9a-fA-F]{4}|.)", rep, s)


def used_keys() -> set[str]:
    """Keys passed to t("...") literally, plus `key: "..."` fields of the UI's data tables
    (menus, toolbars), which reach t() through the table."""
    keys: set[str] = set()
    for ts in (ROOT / "ui/src").rglob("*.ts"):
        text = ts.read_text(encoding="utf-8")
        keys.update(re.findall(r"\bt\(\s*\"([^\"]+)\"", text))
        keys.update(re.findall(r"\"?\bkey\"?:\s*\"([^\"]+)\"", text))
    return keys


def main() -> None:
    java = Path(sys.argv[1])
    sha = subprocess.run(["git", "-C", str(java), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    if sha != FIXED_SHA:
        raise SystemExit(f"{java} is at {sha or '?'}, expected {FIXED_SHA}")
    l10n = java / "src/main/resources/l10n"
    base = load_properties(l10n / "DisplayStrings.properties")
    keys = used_keys()
    result: dict[str, dict[str, str]] = {}
    missing: set[str] = set()
    for locale, file in LOCALES.items():
        bundle = load_properties(l10n / file)
        table = {}
        for k in sorted(keys):
            if k in bundle:
                table[k] = bundle[k]
            elif k in base:
                table[k] = base[k]
            else:
                missing.add(k)
        result[locale] = table
    out = ROOT / "ui/src/i18n/javaStrings.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"source": f"wimi321/lizzieyzy-next {FIXED_SHA} src/main/resources/l10n", "strings": result},
                              ensure_ascii=False, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    # Keys that are literal Java text (language names, Chinese-only items) are expected here.
    print(f"{len(keys)} keys, {len(missing)} without a bundle entry (shown literally):")
    for k in sorted(missing):
        print("  ", k)


if __name__ == "__main__":
    main()

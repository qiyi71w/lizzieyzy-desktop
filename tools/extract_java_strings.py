#!/usr/bin/env python3
"""Regenerate ui/src/i18n/javaStrings.json from the fixed Java resource bundles.

The UI shows Java's own wording. This script collects every resource key the UI uses (`t("Key")`
calls and `"key"` entries in ui/src/chrome/javaMenus.ts) and copies those entries from the Java
DisplayStrings bundles, per locale, falling back to the base bundle like Java's ResourceBundle.
CORRECTIONS replaces the few Java entries that are wrong, per locale.

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

# Java entries this product corrects. 目数显示's items read as the opposite score in English,
# Korean and Thai (Menu.leadWithKomi shows KataGo's lead after komi, Menu.leadJustScore the board
# difference); they follow the Japanese and the Chinese, with the search aliases naming them. The
# Japanese update button was a request, the Yike sync failure was slang in three locales, and the
# proxy subtitle named Java.
CORRECTIONS: dict[str, dict[str, str]] = {
    "ZhCn": {
        "ConfigDialog2.modern.proxy.subtitle": "控制程序自身网络请求使用的代理策略",
    },
    "ZhTw": {
        "ConfigDialog2.modern.proxy.subtitle": "選擇來自應用程式的網路請求如何使用代理",
        "YikeLiveDialog.syncFailed": "弈客同步失敗",
    },
    "EnUs": {
        "ConfigDialog2.modern.proxy.subtitle": "Choose how network requests from the app use a proxy",
        "FunctionSearch.aliases.menu.kataScolreLead": "Show score lead|Including komi|Excluding komi",
        "Menu.leadJustScore": "Excluding komi",
        "Menu.leadWithKomi": "Including komi",
    },
    "JaJp": {
        "ConfigDialog2.modern.proxy.subtitle": "アプリからのネットワーク リクエストでプロキシを使用する方法を選択します",
        "WindowsUpdate.btnUpdate": "今すぐ更新",
        "YikeLiveDialog.syncFailed": "弈客の同期に失敗しました",
    },
    "KoKr": {
        "ConfigDialog2.modern.proxy.subtitle": "앱의 네트워크 요청이 프록시를 사용하는 방법을 선택하세요.",
        "FunctionSearch.aliases.menu.kataScolreLead": "우세 집수 표시|점수 리드 보기|덤 포함|덤 제외",
        "Menu.leadJustScore": "덤 제외",
        "Menu.leadWithKomi": "덤 포함",
        "YikeLiveDialog.syncFailed": "Yike 동기화 실패",
    },
    "ThTh": {
        "ConfigDialog2.modern.proxy.subtitle": "เลือกวิธีที่คำขอเครือข่ายจากแอปใช้พร็อกซี",
        "FunctionSearch.aliases.menu.kataScolreLead": "แสดงคะแนนนำ|รวมโคมิ|ไม่รวมโคมิ",
        "Menu.leadJustScore": "ไม่รวมโคมิ",
        "Menu.leadWithKomi": "รวมโคมิ",
    },
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
    (menus, toolbars), which reach t() through the table, and keys the Rust core writes into
    records through `java_text("...")`."""
    keys: set[str] = set()
    for ts in (ROOT / "ui/src").rglob("*.ts"):
        text = ts.read_text(encoding="utf-8")
        keys.update(re.findall(r"\bt\(\s*\"([^\"]+)\"", text))
        keys.update(re.findall(r"\"?\bkey\"?:\s*\"([^\"]+)\"", text))
    for rs in (ROOT / "crates").rglob("*.rs"):
        keys.update(re.findall(r"\bjava_text\(\s*[^,()]+,\s*\"([^\"]+)\"", rs.read_text(encoding="utf-8")))
    catalog = json.loads((ROOT / "crates/lizzie-core/src/function_search/catalog.json").read_text(encoding="utf-8"))
    keys.update(catalog["categories"])
    for entry in catalog["entries"]:
        keys.update(entry[field] for field in ("title", "description", "category", "aliases", "weakAliases") if entry[field])
        keys.update(entry["path"])
    return keys


def main() -> None:
    java = Path(sys.argv[1])
    sha = subprocess.run(["git", "-C", str(java), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    if sha != FIXED_SHA:
        raise SystemExit(f"{java} is at {sha or '?'}, expected {FIXED_SHA}")
    l10n = java / "src/main/resources/l10n"
    base = load_properties(l10n / "DisplayStrings.properties")
    keys = used_keys()
    # A function's aliases and description are wanted only while the catalogue lists the function.
    per_function = ("FunctionSearch.aliases.", "FunctionSearch.description.")
    keys.update(key for key in base if key.startswith("FunctionSearch.") and not key.startswith(per_function))
    # Match summaries and notices select Java keys dynamically by side/outcome; AI 解说 statuses,
    # failures and ChatGPT messages arrive from the core and the adapter as Teacher.* keys; Fox
    # direct-connection statuses and failures arrive from the core as Fox.* keys.
    keys.update(key for key in base if key.startswith(("EngineGameInfo.", "EnginePkConfig.",
        "NewEngineGameDialog.", "Mannul.", "Teacher.", "Fox.")))
    keys.add("GtpConsolePane.isEngineGame")
    bundles = {locale: load_properties(l10n / file) for locale, file in LOCALES.items()}
    for bundle in bundles.values():
        keys.update(key for key in bundle if key.startswith("FunctionSearch.") and not key.startswith(per_function))
    result: dict[str, dict[str, str]] = {}
    missing: set[str] = set()
    for locale, bundle in bundles.items():
        table = {}
        for k in sorted(keys):
            if k in bundle:
                table[k] = bundle[k]
            elif k in base:
                table[k] = base[k]
            else:
                missing.add(k)
        table.update(CORRECTIONS.get(locale, {}))
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

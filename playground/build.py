#!/usr/bin/env python3
"""Embed the WebAssembly build of elfcalc into the playground page.
Usage: build.py WASM_FILE OUT_HTML [--standalone]
  default       page body for hosts that add their own document skeleton (Claude artifacts)
  --standalone  a complete HTML document (doctype, charset, mobile viewport) for GitHub Pages or any web server
Build the wasm with: cargo build --release --lib --target wasm32-unknown-unknown"""
import base64, os, sys
args = [a for a in sys.argv[1:] if not a.startswith("--")]
wasm, out = args
tpl = open(os.path.join(os.path.dirname(__file__), "template.html"), encoding="utf-8").read()
assert tpl.count("__WASM_BASE64__") == 1
html = tpl.replace("__WASM_BASE64__", base64.b64encode(open(wasm, "rb").read()).decode())
if "--standalone" in sys.argv:
    head, sep, rest = html.partition("<style>")
    html = ('<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n'
            '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
            + head + sep + rest.replace("</style>", "</style>\n</head>\n<body>", 1) + "\n</body>\n</html>\n")
open(out, "w", encoding="utf-8").write(html)
print(f"wrote {out} ({len(html) // 1024} KB){' standalone' if '--standalone' in sys.argv else ''}")

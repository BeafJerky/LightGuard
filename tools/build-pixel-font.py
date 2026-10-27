from __future__ import annotations

import argparse
from pathlib import Path

from fontTools.subset import Options, Subsetter
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
PROTO = ROOT.parent / "docs" / "prototype"
OUT = PROTO / "fonts" / "lg-pixel-12px-subset.woff2"
CJK_SOURCES = ["index.html", "app.js", "styles.css", "desktop.css", "themes.css"]
EXTRA = "，。、：；！？（）【】「」『』《》〈〉·—…“”‘’￥→←↑↓×÷°±≤≥≈∞•"


def collect_chars() -> str:
    chars = {chr(code) for code in range(0x20, 0x7F)}
    chars.update(EXTRA)
    for name in CJK_SOURCES:
        path = PROTO / name
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8")
        chars.update(ch for ch in text if "\u4e00" <= ch <= "\u9fff")
        chars.update(ch for ch in text if ch in EXTRA)
    return "".join(sorted(chars))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", nargs="?", default="/tmp/lg-font/source.woff2")
    args = parser.parse_args()

    source = Path(args.source)
    if not source.exists():
        raise SystemExit(f"missing source font: {source}")

    text = collect_chars()
    options = Options()
    options.layout_features = ["*"]
    options.name_IDs = ["*"]
    options.name_legacy = True
    options.name_languages = ["*"]
    options.notdef_outline = True
    options.recalc_bounds = True
    options.desubroutinize = True

    font = TTFont(source)
    subsetter = Subsetter(options=options)
    subsetter.populate(text=text)
    subsetter.subset(font)

    for record in font["name"].names:
        if record.nameID in (1, 3, 4, 6, 16, 17):
            record.string = "LG Pixel"

    font.flavor = "woff2"
    OUT.parent.mkdir(parents=True, exist_ok=True)
    font.save(OUT)

    size = OUT.stat().st_size
    print(f"chars={len(set(text))} size={size} bytes -> {OUT}")
    if size > 80 * 1024:
        raise SystemExit("subset exceeds 80KB; reduce charset or report upstream")


if __name__ == "__main__":
    main()

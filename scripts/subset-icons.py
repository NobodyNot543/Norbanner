"""Builds the bundled icon font from the full Material Symbols Rounded font.

Keeps only the glyphs `apps/shell/src/icons.rs` names, pins the optical size
and grade axes, and keeps the FILL and weight axes the shell animates. The
full font is about 15 MB; the subset a few dozen KB.

Usage (needs `pip install fonttools`):
    python scripts/subset-icons.py <MaterialSymbolsRounded[FILL,GRAD,opsz,wght].ttf>
        [<...codepoints file>]

Without arguments both files are downloaded from google/material-design-icons.
"""

import pathlib
import re
import sys
import tempfile
import urllib.request

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICONS = ROOT / "apps" / "shell" / "src" / "icons.rs"
OUT = ROOT / "assets" / "fonts" / "MaterialSymbolsRounded.ttf"
BASE = "https://github.com/google/material-design-icons/raw/master/variablefont/"
NAME = "MaterialSymbolsRounded%5BFILL,GRAD,opsz,wght%5D"


def fetch(url: str, into: pathlib.Path) -> pathlib.Path:
    urllib.request.urlretrieve(url, into)
    return into


def main() -> None:
    work = pathlib.Path(tempfile.mkdtemp())
    font_path = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else fetch(BASE + NAME + ".ttf", work / "full.ttf")
    points_path = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else fetch(BASE + NAME + ".codepoints", work / "codepoints")

    catalogue = dict(line.split() for line in points_path.read_text().splitlines() if line.strip())
    wanted = re.findall(r"u\{([0-9a-f]+)\}'; // (\w+)", ICONS.read_text(encoding="utf-8"))
    for code, name in wanted:
        if catalogue.get(name) != code:
            sys.exit(f"{name}: icons.rs says {code}, the catalogue says {catalogue.get(name)}")
    unicodes = sorted({int(code, 16) for code, _ in wanted})

    font = TTFont(font_path)
    options = subset.Options()
    options.layout_features = []
    options.name_IDs = ["*"]
    options.notdef_outline = True
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=unicodes)
    subsetter.subset(font)
    # Instancing after subsetting: the full font's .notdef has no variations,
    # which the instancer does not mind but the subsetter does.
    font = instancer.instantiateVariableFont(font, {"opsz": 24, "GRAD": 0})
    OUT.parent.mkdir(parents=True, exist_ok=True)
    font.save(OUT)
    print(f"{len(unicodes)} glyphs -> {OUT} ({OUT.stat().st_size // 1024} KiB)")


if __name__ == "__main__":
    main()

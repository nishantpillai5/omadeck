#!/usr/bin/env python3
"""Regenerate `crates/omadeck-core/tests/fixtures/test-font.ttf`.

The test font is a JetBrains Mono Bold subset (OFL). On top of its printable-ASCII
glyphs it carries the Nerd Font glyphs the renderer draws (the `*_GLYPH` constants in
`crates/omadeck-core/src/render.rs`), so glyph rendering in the render tests is
self-contained and never falls back to the machine's fonts.

Those codepoints are in the Supplementary Private Use Area-B (U+F0xxx, above U+FFFF),
so the font needs a format 12 cmap subtable to map them.

Usage:
    python3 scripts/make-test-font.py [PATH-TO-NERD-FONT]

Needs `fonttools` (`pip install fonttools`). The Nerd Font is any JetBrains Mono Nerd
Font regular weight; defaults are probed in common locations. The output is written
straight over the fixture, so commit the result.
"""
import os
import sys

try:
    from fontTools.ttLib import TTFont
    from fontTools.pens.recordingPen import RecordingPen
    from fontTools.pens.ttGlyphPen import TTGlyphPen
    from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
except ImportError:
    sys.exit("fonttools is required: pip install fonttools")

TEST_FONT = os.path.join(os.path.dirname(__file__), "..", "crates", "omadeck-core",
                         "tests", "fixtures", "test-font.ttf")

# Keep in sync with the `*_GLYPH` constants in render.rs.
CODEPOINTS = [
    0xF0079, 0xF0084, 0xF00DE, 0xF0141, 0xF0142, 0xF0150, 0xF018D,
    0xF035B, 0xF036C, 0xF0387, 0xF03E4, 0xF040A, 0xF0493, 0xF056E,
    0xF057E, 0xF059F, 0xF061A, 0xF075F, 0xF08C6,
]

DEFAULT_NERD_FONTS = [
    "/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Regular.ttf",
    "/usr/share/fonts/JetBrainsMonoNerdFont-Regular.ttf",
    os.path.expanduser("~/.local/share/fonts/JetBrainsMonoNerdFont-Regular.ttf"),
    os.path.expanduser("~/.fonts/JetBrainsMonoNerdFont-Regular.ttf"),
]


def main() -> int:
    nerd = sys.argv[1] if len(sys.argv) > 1 else next(
        (p for p in DEFAULT_NERD_FONTS if os.path.exists(p)), None)
    if nerd is None:
        sys.exit("no Nerd Font found; pass a JetBrainsMono Nerd Font path as an argument")

    src = TTFont(nerd)
    dst = TTFont(TEST_FONT)
    src_cmap = src.getBestCmap()
    # The combined map (every subtable): a codepoint may live in the format 4 (BMP)
    # or the format 12 (Supplementary PUA-B) subtable.
    dst_cmap = {}
    for st in dst["cmap"].tables:
        dst_cmap.update(st.cmap)
    glyf = dst["glyf"]
    glyph_order = dst.getGlyphOrder()

    # A format 12 subtable holds the Supplementary PUA-B codepoints (U+F0xxx, > U+FFFF).
    f12 = next((st for st in dst["cmap"].tables if st.format == 12), None)
    if f12 is None:
        f12 = CmapSubtable.newSubtable(12)
        f12.platformID, f12.platEncID, f12.language = 3, 10, 0
        f12.cmap = {}
        dst["cmap"].tables.append(f12)

    added, skipped, missing = [], [], []
    for cp in CODEPOINTS:
        if cp in dst_cmap:
            skipped.append(cp)
            continue
        gname = src_cmap.get(cp)
        if gname is None:
            missing.append(cp)
            continue
        rec = RecordingPen()
        src["glyf"][gname].draw(rec, src["glyf"])
        pen = TTGlyphPen(None)
        rec.replay(pen)
        glyph = pen.glyph()
        glyph.width = src.getGlyphSet()[gname].width
        newname = f"uni{cp:04X}"
        if newname in glyph_order:
            newname = f"g{cp:05X}"
        glyph_order.append(newname)
        glyf[newname] = glyph
        dst["hmtx"].metrics[newname] = (glyph.width, 0)
        f12.cmap[cp] = newname
        added.append(cp)

    dst.save(TEST_FONT)
    print(f"source Nerd Font: {nerd}")
    print(f"added {len(added)} glyphs: " + ", ".join(f"U+{c:05X}" for c in added))
    if skipped:
        print(f"skipped {len(skipped)} (already present): " + ", ".join(f"U+{c:05X}" for c in skipped))
    if missing:
        print(f"WARNING: not in source font: " + ", ".join(f"U+{c:05X}" for c in missing),
              file=sys.stderr)

    # Verify.
    chk = TTFont(TEST_FONT)
    full = {}
    for st in chk["cmap"].tables:
        full.update(st.cmap)
    absent = [cp for cp in CODEPOINTS if cp not in full]
    if absent:
        sys.exit(f"verification failed, still missing: " + ", ".join(f"U+{c:05X}" for c in absent))
    print(f"verified: all {len(CODEPOINTS)} glyphs present ({len(full)} codepoints total)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

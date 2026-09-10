#!/usr/bin/env bash
# GölgeDosya — marka varlıklarından uygulama ikonlarını üretir.
#
# TEK KAYNAK: brand/appicon.svg. src-tauri/icons/ altındaki her şey buradan
# türetilir; o dizin elle düzenlenmez. Marka değişirse yalnız SVG değişir ve
# bu betik yeniden çalıştırılır.
#
# Araçlar macOS'un kendisinden gelir (sips, iconutil); ek bağımlılık yok.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$HERE/appicon.svg"
OUT="$HERE/../src-tauri/icons"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

png() { sips -s format png -Z "$1" "$SRC" --out "$2" >/dev/null; }

mkdir -p "$OUT"
png 512  "$OUT/icon.png"
png 32   "$OUT/32x32.png"
png 64   "$OUT/64x64.png"
png 128  "$OUT/128x128.png"
png 256  "$OUT/128x128@2x.png"
png 50   "$OUT/StoreLogo.png"
for s in 30 44 71 89 107 142 150 284 310; do
  png "$s" "$OUT/Square${s}x${s}Logo.png"
done

# macOS .icns — iconutil'in beklediği ad şeması.
ICONSET="$TMP/GolgeDosya.iconset"
mkdir -p "$ICONSET"
for pair in "16 icon_16x16" "32 icon_16x16@2x" "32 icon_32x32" "64 icon_32x32@2x" \
            "128 icon_128x128" "256 icon_128x128@2x" "256 icon_256x256" \
            "512 icon_256x256@2x" "512 icon_512x512" "1024 icon_512x512@2x"; do
  set -- $pair
  png "$1" "$ICONSET/$2.png"
done
iconutil -c icns "$ICONSET" -o "$OUT/icon.icns"

# Windows .ico — PNG taşıyan basit bir kap; harici araç gerektirmez.
for s in 16 24 32 48 64 128 256; do png "$s" "$TMP/ico-$s.png"; done
python3 - "$TMP" "$OUT/icon.ico" <<'PY'
import struct, sys, os
tmp, out = sys.argv[1], sys.argv[2]
sizes = [16, 24, 32, 48, 64, 128, 256]
blobs = [open(os.path.join(tmp, f"ico-{s}.png"), "rb").read() for s in sizes]
header = struct.pack("<HHH", 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
entries, data = b"", b""
for s, b in zip(sizes, blobs):
    entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(b), offset)
    offset += len(b)
    data += b
open(out, "wb").write(header + entries + data)
PY

echo "üretildi: $(ls "$OUT" | wc -l | tr -d ' ') dosya → $OUT"

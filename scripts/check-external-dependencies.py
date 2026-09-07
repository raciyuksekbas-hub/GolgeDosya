#!/usr/bin/env python3
"""Birleşik deponun kendi dışına uzanan derleme bağımlılıkları.

Migration boyunca motorlar bağımsız depolarda kaldı: tek kaynak/iki tüketici
modeli parity'yi tanım gereği garantiliyordu. Final mimaride bu kabul edilemez —
taze bir klon kendi başına derlenebilmelidir.

Bu bir MANDALDIR: sayı artarsa kapı kapanır, azaldığında da kapanır (mandalı
düşürmeyi unutmayın). Hedef sıfırdır ve final consolidation'ın çıkış kapısıdır.
"""

import pathlib
import re
import sys

# 2026-09-07 · belge-shell→ekler-core, belge-shell→ikincigoz-core,
# preflight→ekler-core. Azaldıkça bu sayı da düşürülmelidir.
BASELINE = 3


def main() -> int:
    root = pathlib.Path(__file__).resolve().parent.parent
    external = []
    for manifest in sorted(root.rglob("Cargo.toml")):
        if "target" in manifest.parts or "node_modules" in manifest.parts:
            continue
        for line in manifest.read_text().splitlines():
            m = re.search(r'path = "([^"]+)"', line)
            if not m:
                continue
            # Yolu gerçekten çöz: "../../../crates/document-core" depo İÇİNDEDİR,
            # metne bakarak karar vermek yanıltır.
            resolved = (manifest.parent / m.group(1)).resolve()
            if root not in resolved.parents:
                external.append(
                    f"{manifest.relative_to(root)}: {line.strip()}  →  {resolved}"
                )

    print(f"depo dışı derleme bağımlılığı: {len(external)} (mandal: {BASELINE})")
    for e in external:
        print("  " + e)

    if len(external) > BASELINE:
        print("YENİ dış bağımlılık — final mimari hedefi ters yöne gidiyor.", file=sys.stderr)
        return 1
    if len(external) < BASELINE:
        print(
            f"Bağımlılık azalmış. BASELINE değerini {len(external)} yapın.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

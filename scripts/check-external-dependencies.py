#!/usr/bin/env python3
"""Birleşik deponun kendi dışına uzanan derleme bağımlılıkları.

Migration boyunca motorlar bağımsız depolarda kaldı: tek kaynak/iki tüketici
modeli parity'yi tanım gereği garantiliyordu. Final mimaride bu kabul edilemez —
taze bir klon kendi başına derlenebilmelidir.

Bu bir MANDALDIR ve mandal artık SIFIRDA: tek bir dış yol bağımlılığı eklemek
kapıyı kapatır. Taze klon → kur → derle → test sözü doğrudan buna dayanır.
"""

import pathlib
import re
import sys

# 2026-09-08 · SIFIR. Final consolidation'da üç dış yol bağımlılığının tamamı
# kaldırıldı; `ekler-core` ve `ikincigoz-core` artık bu deponun içinde canonical
# source. Bu sayı bir daha ARTMAMALI: taze klon → derle → test sözü buna bağlı.
BASELINE = 0


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

    # Manifest yetmez. Taze klon kanıtı, `ikincigoz_parity_tests.rs`'in
    # KAYNAK KOD içinden bağımsız depoya uzandığını ortaya çıkardı: manifest'te
    # yol bağımlılığı yoktu, klonda beş test düşüyordu. Bu yüzden kaynak da
    # taranır.
    #
    # `#[ignore]` ile işaretli testler hariç: onlar sahibinin makinesindeki özel
    # fikstürlere bilerek bağlı ve derlemeyi ya da klon kanıtını etkilemiyorlar.
    source_refs = []
    old_repos = ("İkinciGöz/", "DuzenEk/", "Documents/Tavzih/", "Degisik-Is/")
    for src in sorted(root.rglob("*.rs")):
        parts = src.parts
        if "target" in parts or "node_modules" in parts:
            continue
        text = src.read_text(errors="replace")
        ignored = "#[ignore" in text
        for n, line in enumerate(text.splitlines(), 1):
            stripped = line.strip()
            if stripped.startswith("//"):
                continue
            if any(r in line for r in old_repos):
                if ignored:
                    continue
                source_refs.append(f"{src.relative_to(root)}:{n}: {stripped[:90]}")

    print(f"depo dışı derleme bağımlılığı: {len(external)} (mandal: {BASELINE})")
    if source_refs:
        print(f"KAYNAK KODDA depo dışı yol: {len(source_refs)}")
        for r in source_refs:
            print("  " + r)
        print(
            "Kaynak koddan eski depoya uzanan yol, taze klonu bozar.",
            file=sys.stderr,
        )
        return 1
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

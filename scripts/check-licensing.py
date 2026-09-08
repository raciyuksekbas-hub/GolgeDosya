#!/usr/bin/env python3
"""Lisans beyanı tutarlılığı.

Birleşme, dört bağımsız deponun kodunu tek workspace altında topladı. Bu
depolar AYNI lisansı beyan etmiyordu:

    Tavzih      MIT
    DüzenEk     MIT
    İkinciGöz   Proprietary
    Değişikİş   (Rust workspace lisans satırı yok)

Birleşik workspace `Proprietary` beyan ediyor, dolayısıyla `license.workspace`
kullanan her crate Proprietary oluyor. Bu, MIT beyan eden iki deponun motor
kodunun beyanını sessizce değiştirmek anlamına gelir.

Bu bir MÜHENDİSLİK kararı değildir; sahibinin kararıdır. Bu betik karar vermez,
tutarsızlığı görünür tutar ve sessizce kaymasını engeller.

Çıkış kodu 0 = beklenen durum; 1 = beyanlar beklenenden farklı.
"""

import pathlib
import re
import sys

# Bilinen ve KABUL EDİLMİŞ durum. Bir crate'in beyanı değişirse kapı kapanır ve
# değişiklik bilinçli olarak buraya yazılmak zorunda kalır.
EXPECTED = {
    "document-core": "MIT",          # Tavzih'ten geldi, kendi beyanını koruyor
    "pdf-core": "workspace",
    "ekler-core": "workspace",
    "ikincigoz-core": "workspace",
    "process-bridge": "workspace",
}
WORKSPACE_LICENSE = "Proprietary"


def declared(manifest: pathlib.Path) -> str:
    for line in manifest.read_text().splitlines():
        m = re.match(r'\s*license\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
        if re.match(r"\s*license\.workspace\s*=\s*true", line):
            return "workspace"
        if re.match(r"\s*license\s*=\s*\{\s*workspace\s*=\s*true", line):
            return "workspace"
    return "BEYAN YOK"


def main() -> int:
    root = pathlib.Path(__file__).resolve().parent.parent
    ws = declared(root / "Cargo.toml")
    print(f"workspace lisansı: {ws}")
    if ws != WORKSPACE_LICENSE:
        print(f"  workspace beyanı {WORKSPACE_LICENSE} bekleniyordu", file=sys.stderr)
        return 1

    bad = False
    for crate, expect in sorted(EXPECTED.items()):
        manifest = root / "crates" / crate / "Cargo.toml"
        if not manifest.is_file():
            print(f"  {crate}: manifest yok", file=sys.stderr)
            bad = True
            continue
        got = declared(manifest)
        effective = ws if got == "workspace" else got
        flag = "✓" if got == expect else "✗"
        print(f"  {flag} {crate:<16} beyan={got:<12} etkin={effective}")
        if got != expect:
            bad = True

    if bad:
        print(
            "\nLisans beyanı beklenenden farklı. Bu kapıyı gevşetmeden önce\n"
            "sahibinin kararını alın ve EXPECTED tablosunu güncelleyin.",
            file=sys.stderr,
        )
        return 1

    # Deponun kendi LICENSE dosyası — dört bağımsız depoda vardı, burada yok.
    if not any((root / n).exists() for n in ("LICENSE", "LICENSE.md", "LICENSE.txt")):
        print("\n  ! Bu depoda LICENSE dosyası yok (dört bağımsız depoda vardı).")
        print("    Yayın öncesi kapatılması gereken açık bir madde.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

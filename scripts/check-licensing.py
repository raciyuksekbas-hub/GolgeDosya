#!/usr/bin/env python3
"""Lisans rejimi denetimi — fail-closed.

Sahibinin 2026-09-08 kararı: birleşik ürün ve bütün birinci taraf crate'ler
**Proprietary / All Rights Reserved**.

Provenance denetimi (2026-09-08) birinci taraf bileşenlerin hiçbirinde gömülü
üçüncü taraf kaynak kod bulunmadığını gösterdi: telif başlığı yok, SPDX
bildirimi yok, "adapted/derived/ported from" işareti yok, vendored dosya yok.
`document-core/src/udf/mod.rs` kara kutu gözlemine dayandığını açıkça beyan
eder. Bu yüzden hepsi Proprietary olarak standardize edilebildi.

Bağımsız Tavzih ve DüzenEk depolarının YAYIMLANMIŞ MIT sürümleri bu kararla
değişmez; burada denetlenen yalnız birleşik ürünün rejimidir.

Bu betik beş şeyi denetler ve herhangi biri tutmazsa çıkış kodu 1 verir:
  1. workspace lisansı Proprietary
  2. her birinci taraf crate ya workspace'ten miras alır ya Proprietary beyan eder
  3. depo kökünde LICENSE var
  4. depo kökünde THIRD_PARTY_NOTICES.md var
  5. hiçbir bağımlılık GPL/AGPL değil ve lisansı bilinmeyen paket yok
"""

import json
import pathlib
import re
import subprocess
import sys

WORKSPACE_LICENSE = "Proprietary"

FIRST_PARTY = [
    "crates/document-core",
    "crates/pdf-core",
    "crates/ekler-core",
    "crates/ikincigoz-core",
    "crates/process-bridge",
    "apps/belge-shell/src-tauri",
    "tools/preflight",
]

ok_count = 0
bad = []


def ok(msg):
    global ok_count
    print(f"  \033[32m✓\033[0m {msg}")
    ok_count += 1


def fail(msg):
    print(f"  \033[31m✗\033[0m {msg}")
    bad.append(msg)


def declared(manifest: pathlib.Path) -> str:
    for line in manifest.read_text().splitlines():
        if line.lstrip().startswith("#"):
            continue
        m = re.match(r'\s*license\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
        if re.match(r"\s*license(\.workspace\s*=\s*true|\s*=\s*\{\s*workspace\s*=\s*true)", line):
            return "workspace"
    return "BEYAN YOK"


def main() -> int:
    root = pathlib.Path(__file__).resolve().parent.parent
    print("\n\033[1mLİSANS REJİMİ\033[0m")

    # 1 — workspace
    ws = declared(root / "Cargo.toml")
    if ws == WORKSPACE_LICENSE:
        ok(f"workspace lisansı: {ws}")
    else:
        fail(f"workspace lisansı {WORKSPACE_LICENSE} olmalı, {ws} bulundu")

    # 2 — birinci taraf crate'ler
    inconsistent = []
    for rel in FIRST_PARTY:
        manifest = root / rel / "Cargo.toml"
        if not manifest.is_file():
            inconsistent.append(f"{rel}: manifest yok")
            continue
        got = declared(manifest)
        if got not in ("workspace", WORKSPACE_LICENSE):
            inconsistent.append(f"{rel}: {got}")
    if inconsistent:
        fail("birinci taraf lisans metadata tutarsız: " + "; ".join(inconsistent))
    else:
        ok(f"birinci taraf {len(FIRST_PARTY)} crate: hepsi {WORKSPACE_LICENSE}")

    # 3 — LICENSE
    lic = next((root / n for n in ("LICENSE", "LICENSE.md", "LICENSE.txt") if (root / n).is_file()), None)
    if lic is None:
        fail("depo kökünde LICENSE dosyası yok")
    elif "Tüm hakları saklıdır" not in lic.read_text():
        fail(f"{lic.name} bir mülkiyet lisansı gibi görünmüyor")
    else:
        ok(f"{lic.name} mevcut (mülkiyet)")

    # 4 — üçüncü taraf bildirimleri
    notices = root / "THIRD_PARTY_NOTICES.md"
    if not notices.is_file():
        fail("THIRD_PARTY_NOTICES.md yok")
    else:
        text = notices.read_text()
        missing = [k for k in ("MPL-2.0", "jszip", "pdfjs-dist", "mammoth") if k not in text]
        if missing:
            fail("THIRD_PARTY_NOTICES.md eksik kayıt: " + ", ".join(missing))
        else:
            ok("THIRD_PARTY_NOTICES.md mevcut ve dikkat gerektiren kayıtları taşıyor")

    # 5 — bağımlılık lisansları (fail-closed)
    try:
        meta = json.loads(
            subprocess.run(
                ["cargo", "metadata", "--format-version", "1"],
                capture_output=True, text=True, cwd=root, check=True,
            ).stdout
        )
    except Exception as e:
        fail(f"cargo metadata çalıştırılamadı: {e}")
        meta = None

    if meta:
        copyleft, unknown = [], []
        for p in meta["packages"]:
            if p.get("source") is None:
                continue
            lic = (p.get("license") or "").strip()
            if not lic and not p.get("license_file"):
                unknown.append(p["name"])
                continue
            u = lic.upper()
            # İzin verici bir seçenek sunan çoklu lisans kabul edilir.
            if " OR " in u or "/" in u:
                continue
            if re.search(r"\bA?GPL-", u) and "LGPL" not in u:
                copyleft.append(f"{p['name']} ({lic})")
        if unknown:
            fail("lisansı bilinmeyen Rust paketi: " + ", ".join(sorted(set(unknown))))
        elif copyleft:
            fail("GPL/AGPL Rust paketi: " + ", ".join(sorted(set(copyleft))))
        else:
            n = sum(1 for p in meta["packages"] if p.get("source"))
            ok(f"Rust: {n} bağımlılık, GPL/AGPL yok, bilinmeyen lisans yok")

    print()
    if bad:
        print("\033[31m  LİSANS DENETİMİ BAŞARISIZ\033[0m\n")
        return 1
    print(f"\033[32m  LİSANS DENETİMİ GEÇTİ ({ok_count} kontrol)\033[0m\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())

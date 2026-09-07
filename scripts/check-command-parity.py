#!/usr/bin/env python3
"""Taşınan komutların gövde eşitliği.

Bağımsız DüzenEk'in `src-tauri/src/lib.rs` dosyasındaki 21 `#[tauri::command]`
ile birleşik uygulamanın `modules/duzenek.rs` dosyasındaki karşılıklarını
karşılaştırır. Beklenen tek fark komut adının `duzenek_` önekli olmasıdır.

Neden gövde karşılaştırması: bağımsız uygulamanın komutları `pub` değil, yani
dışarıdan çağrılamaz ve baseline dondurulmuş olduğu için `pub` yapılmayacak. İki
binary'yi yan yana çalıştırmak yerine, komut katmanının kaynak olarak aynı
olduğu ve motorun (ekler-core) tek kopya olduğu kanıtlanır. Aynı girdi + aynı
kod + aynı motor = aynı çıktı. Davranışsal ölçüm ayrı bir kapıdır
(`tests/duzenek_parity.rs`).

Çıkış kodu 0 = tüm gövdeler aynı.
"""

import re
import sys
from pathlib import Path

PREFIX = "duzenek_"


def commands(source: str) -> dict[str, str]:
    """`#[tauri::command]` ile işaretli her fonksiyonun imzası ve gövdesi."""
    found: dict[str, str] = {}
    for match in re.finditer(r"#\[tauri::command\]\s*\n", source):
        start = match.end()
        name = re.search(r"\bfn\s+([a-z0-9_]+)", source[start : start + 200])
        if not name:
            continue
        brace = source.index("{", start + name.end())
        depth, i = 0, brace
        while i < len(source):
            if source[i] == "{":
                depth += 1
            elif source[i] == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        found[name.group(1)] = source[start : i + 1]
    return found


def normalise(text: str, name: str) -> str:
    """Ada ve görünürlüğe bağlı farkları eler; gövde aynen kalır."""
    text = text.replace(PREFIX + name, name)
    text = re.sub(r"\bpub(\(crate\))?\s+(async\s+)?fn\b", r"\2fn", text)
    return "\n".join(line.rstrip() for line in text.strip().splitlines())


def semantic(text: str, name: str) -> str:
    """Boşluğa duyarsız biçim.

    Önek adları uzattığı için rustfmt bazı imzaları çok satıra sardı. Rust'ta
    boşluk anlam taşımaz, dolayısıyla kapı bu biçimde karşılaştırır. Ham metin
    farkı ayrıca bilgi olarak raporlanır — sessizce yutulmaz.
    """
    flat = " ".join(normalise(text, name).split())
    # rustfmt çok satıra sardığı yerlere sondaki virgülü ekler ve parantez
    # içine boşluk bırakır; Rust'ta ikisi de anlam taşımaz.
    flat = re.sub(r",\s*([)\]}])", r"\1", flat)
    flat = re.sub(r"([(\[])\s+", r"\1", flat)
    return re.sub(r"\s+([)\]])", r"\1", flat)


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    standalone = Path.home() / "Projects/DuzenEk/src-tauri/src/lib.rs"
    unified = root / "apps/belge-shell/src-tauri/src/modules/duzenek.rs"
    if not standalone.is_file():
        print(f"bağımsız kaynak bulunamadı: {standalone}", file=sys.stderr)
        return 2

    a = commands(standalone.read_text())
    b = {k[len(PREFIX) :]: v for k, v in commands(unified.read_text()).items() if k.startswith(PREFIX)}

    missing = sorted(set(a) - set(b))
    extra = sorted(set(b) - set(a))
    differing, rewrapped = [], []
    for name in sorted(set(a) & set(b)):
        if semantic(a[name], name) != semantic(b[name], name):
            differing.append(name)
        elif normalise(a[name], name) != normalise(b[name], name):
            rewrapped.append(name)

    print(f"bağımsız komut: {len(a)}   birleşik komut: {len(b)}")
    if missing:
        print("TAŞINMAMIŞ: " + ", ".join(missing))
    if extra:
        print("BAĞIMSIZDA YOK: " + ", ".join(extra))
    for name in differing:
        print(f"GÖVDE FARKLI: {name}")
        import difflib

        diff = difflib.unified_diff(
            normalise(a[name], name).splitlines(),
            normalise(b[name], name).splitlines(),
            "bağımsız",
            "birleşik",
            lineterm="",
        )
        for line in list(diff)[:40]:
            print("    " + line)

    if missing or extra or differing:
        return 1
    print(f"{len(a)}/{len(a)} komut gövdesi anlamca aynı")
    if rewrapped:
        print(
            "  (yalnız rustfmt sarması farklı, önek adları uzattığı için: "
            + ", ".join(rewrapped)
            + ")"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())

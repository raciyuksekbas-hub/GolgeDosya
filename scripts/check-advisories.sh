#!/usr/bin/env bash
# Bağımlılık açığı kapısı.
#
# 1. deny.toml'daki "bu API çağrılmıyor" gerekçeleri kaynakta denetlenir
#    (fail-closed): biri o API'yi kullanmaya başlarsa gerekçe çöker ve kapı düşer.
#    Aynı taramada lopdf'in yalnız korumalı yükleyiciyle çağrıldığı da
#    denetlenir; bu bir istisna gerekçesi değil, lopdf'in kendisinin
#    kapatmadığı açığın (dolaylı `/Length` zinciri) korumasıdır.
# 2. `cargo deny check advisories` RustSec veritabanına karşı koşar. Gerekçesi
#    yazılmamış her yeni açık kapıyı düşürür.
#
# Çıkış kodu 0 = temiz.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail=0
forbid() {
  local pattern="$1" why="$2" hits
  hits="$(grep -rnE --include='*.rs' "$pattern" crates apps tools 2>/dev/null \
    | grep -v '/target/' || true)"
  if [ -n "$hits" ]; then
    echo "✗ $why"
    echo "$hits" | sed 's/^/    /'
    fail=1
  fi
}
forbid '\.extract\(|ZipStreamReader|read_zipfile_from_stream' \
  "zip extract/stream API kullanılıyor: deny.toml'daki RUSTSEC-2026-0367/0368/0369/0429/0548 gerekçesi geçersiz"
forbid 'by_name_decrypt|by_index_decrypt|aes-crypto' \
  "zip şifre çözme kullanılıyor: RUSTSEC-2026-0545 gerekçesi geçersiz"
forbid 'NsReader' \
  "quick-xml NsReader kullanılıyor: RUSTSEC-2026-0195 gerekçesi geçersiz"
# Denetimli öznitelik okuması karesel (RUSTSEC-2026-0194): her `attributes()`
# aynı ya da bir sonraki satırda (rustfmt zinciri böler) `with_checks(false)`
# almalı ya da tekrar süzen yardımcıdan (`let mut attrs = …; attrs.with_checks`) geçmeli.
attr_hits="$(find crates apps tools -name '*.rs' -not -path '*/target/*' -not -path '*/tests/*' \
  -exec awk '
    prev != "" { if ($0 !~ /with_checks\(false\)/) print prev; prev = "" }
    /\.attributes\(\)/ && !/with_checks\(false\)/ { prev = FILENAME ":" FNR ": " $0 }
    END { if (prev != "") print prev }
  ' {} \; || true)"
if [ -n "$attr_hits" ]; then
  echo "✗ quick-xml attributes() denetimli okunuyor: RUSTSEC-2026-0194 gerekçesi geçersiz"
  echo "$attr_hits" | sed 's/^/    /'
  fail=1
fi
# lopdf yalnız korumalı yükleyiciyle ayrıştırılmalı. lopdf 0.42'den beri iç içe
# yuvalanmayı kendisi sınırlar (RUSTSEC-2026-0187 kapandı); ama dolaylı
# `/Length` başvuru zincirini hâlâ sınırsız özyinelemeyle çözer ve sınırı aşan
# nesneyi sessizce atlar. pdf::guard::load_mem_guarded ikisini de reddeder.
# Desen lopdf 0.47'nin bütün giriş noktalarını yakalar: load, load_mem,
# load_from, *_with_options, *_with_password, load_filtered, load_metadata* ve
# IncrementalDocument::load*. `Reader` 0.47'de dışarıdan kurulamaz. Kendi
# ürettiğimiz çıktıyı yeniden açan yerler ve dağıtılmayan araç istisnadır.
lopdf_hits="$(grep -rnE --include='*.rs' '(LopdfDoc|Document)::load[a-z_]*\(' crates apps 2>/dev/null \
  | grep -v '/target/' | grep -v '/tests/' \
  | grep -vE 'pdf-core/src/pdf/(guard|stamp)\.rs|ekler-core/src/(toolbox|pipeline|raster)\.rs' || true)"
if [ -n "$lopdf_hits" ]; then
  echo "✗ lopdf korumasız yükleniyor: /Length zinciri ve derin nesne akışı reddi atlanıyor (pdf::guard::load_mem_guarded kullanın)"
  echo "$lopdf_hits" | sed 's/^/    /'
  fail=1
fi
[ "$fail" = 0 ] || exit 1
echo "✓ deny.toml gerekçeleri kaynakla tutarlı"

if ! command -v cargo-deny >/dev/null 2>&1; then
  echo "✗ cargo-deny kurulu değil: cargo install --locked cargo-deny"
  exit 1
fi
cargo deny --locked check advisories

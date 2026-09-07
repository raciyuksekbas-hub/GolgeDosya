#!/usr/bin/env bash
#
# Mimari değişmezlerin denetimi. Bir ihlal derlemeyi düşürür.
#
# Bu betik, birleşmenin tek gerçek mimari kuralını kilitler:
#
#     pdf-core -> document-core     SERBEST
#     document-core -> pdf-core     YASAK
#
# Kural bugün zaten geçerli (Tavzih'in tavzih-core'unda tek bir PDF referansı
# yok, DüzenEk'in ekler-core'u tavzih-core'u yalnız 5 API noktasından tüketiyor).
# Yani bu betik yeni bir tasarım dayatmıyor; VAR OLANI koruyor. Kuralı bir kez
# kaybedersek geri kazanmak, hiç kaybetmemekten çok daha pahalıdır.
set -uo pipefail
cd "$(dirname "$0")/.."

FAIL=0
ok()  { printf '  \033[32m✓\033[0m %s\n' "$*"; }
bad() { printf '  \033[31m✗\033[0m %s\n' "$*"; FAIL=1; }

echo
printf '\033[1mMİMARİ DEĞİŞMEZLER\033[0m\n'

# --- 1. document-core PDF bilmemeli --------------------------------------
if [ -d crates/document-core ]; then
  HITS=$(grep -rnIE '\b(lopdf|CoreGraphics|CGPDF|soffice|LibreOffice|sandbox-exec|sips|pdftoppm)\b' \
          crates/document-core/src 2>/dev/null | grep -vc '^\s*//' || true)
  if [ "${HITS:-0}" -gt 0 ]; then
    bad "document-core içinde PDF/renderer sızıntısı ($HITS satır)"
    grep -rnIE '\b(lopdf|CoreGraphics|CGPDF|soffice|LibreOffice|sandbox-exec|sips)\b' crates/document-core/src | sed 's/^/      /'
  else
    ok "document-core: PDF/renderer referansı yok"
  fi

  # Cargo düzeyinde de yasak: yorum satırı hilesi bunu geçemez.
  if grep -qE '^\s*(lopdf|tiff)\b' crates/document-core/Cargo.toml 2>/dev/null; then
    bad "document-core/Cargo.toml PDF bağımlılığı bildiriyor"
  else
    ok "document-core: Cargo'da PDF bağımlılığı yok"
  fi
else
  printf '  \033[33m•\033[0m document-core henüz taşınmadı (Phase 3)\n'
fi

# --- 2. pdf-core -> document-core yönü serbest, ters yön yasak ------------
if [ -d crates/pdf-core ] && [ -d crates/document-core ]; then
  if grep -qE '^\s*pdf-core' crates/document-core/Cargo.toml 2>/dev/null; then
    bad "document-core, pdf-core'a bağımlı — yön TERS"
  else
    ok "document-core, pdf-core'a bağımlı değil"
  fi
else
  printf '  \033[33m•\033[0m pdf-core henüz taşınmadı — bkz. docs/ARCHITECTURE_DECISIONS.md\n'
fi

# --- 3. office-bridge ayrı kalmalı ---------------------------------------
for crate in document-core pdf-core; do
  if [ -d "crates/$crate" ]; then
    HITS=$(grep -rnIE '\b(soffice|LibreOffice|sandbox-exec)\b' "crates/$crate/src" 2>/dev/null | wc -l | tr -d ' ')
    if [ "${HITS:-0}" -gt 0 ]; then
      bad "$crate içinde LibreOffice çağrısı — office-bridge'e ait"
    else
      ok "$crate: LibreOffice çağrısı yok"
    fi
  fi
done

# --- 4. Kabuk motorları doğrudan çağırmamalı -----------------------------
SHELL_SRC=apps/belge-shell/src-tauri/src
HITS=$(grep -rnIE '\b(lopdf|quick_xml|zip::)\b' "$SHELL_SRC" 2>/dev/null | wc -l | tr -d ' ')
if [ "${HITS:-0}" -gt 0 ]; then
  bad "kabuk bir motor kütüphanesini doğrudan kullanıyor"
else
  ok "kabuk: motor kütüphanesi doğrudan kullanılmıyor"
fi

# --- 5. Feature ile capability tutarlılığı -------------------------------
# İzin, kodla birlikte gelir ve kodla birlikte gider. Etkin olmayan bir modülün
# capability dosyası `capabilities/` içinde durursa, kapalı bir modül yüzünden
# izin yüzeyi genişlemiş olur — default-deny'in tam olarak kaybedildiği yer budur.
CARGO="$SHELL_SRC/../Cargo.toml"
CAPS="$SHELL_SRC/../capabilities"
CAPS_PLANNED="$SHELL_SRC/../capabilities-planned"
DEFAULTS=$(sed -n '/^default = /p' "$CARGO")
for m in tavzih duzenek degisikis ikincigoz; do
  ENABLED=0
  printf '%s' "$DEFAULTS" | grep -qE "feature_$m" && ENABLED=1
  ACTIVE=0
  [ -f "$CAPS/feature-$m.json" ] && ACTIVE=1
  PLANNED=0
  [ -f "$CAPS_PLANNED/feature-$m.json" ] && PLANNED=1
  if [ "$ENABLED" = "1" ] && [ "$ACTIVE" = "1" ]; then
    ok "$m: etkin ve capability'si yüklü"
  elif [ "$ENABLED" = "0" ] && [ "$ACTIVE" = "0" ] && [ "$PLANNED" = "1" ]; then
    ok "$m: kapalı, capability'si beklemede"
  elif [ "$ENABLED" = "0" ] && [ "$ACTIVE" = "1" ]; then
    bad "$m: modül kapalı ama capability'si YÜKLÜ — izin yüzeyi bedava genişlemiş"
  elif [ "$ENABLED" = "1" ] && [ "$ACTIVE" = "0" ]; then
    bad "$m: modül etkin ama capability'si yok — çalışma zamanında izin hatası verir"
  else
    bad "$m: capability dosyası hiçbir yerde bulunamadı"
  fi
done

# --- 6. Kabuk ağ istemcisi barındırmamalı --------------------------------
NET=$(grep -rnIE '\b(reqwest|hyper|ureq|TcpStream|UdpSocket)\b' "$SHELL_SRC" 2>/dev/null | wc -l | tr -d ' ')
if [ "${NET:-0}" -gt 0 ]; then
  bad "kabukta ağ istemcisi"
else
  ok "kabukta ağ istemcisi yok"
fi

# --- 7. İzin yüzeyi genişlemesi ------------------------------------------
# capabilities/default.json yalnız kabuğun ihtiyacını taşımalı. Feature izinleri
# capabilities-planned/ altında bekler ve modülüyle birlikte taşınır.
CAP=apps/belge-shell/src-tauri/capabilities/default.json
if grep -qE '"(fs:|shell:|http:)' "$CAP" 2>/dev/null; then
  bad "kabuk capability'sinde fs/shell/http izni"
else
  ok "kabuk capability'si: fs/shell/http izni yok"
fi

echo
if [ "$FAIL" -ne 0 ]; then
  printf '\033[31m  MİMARİ DENETİMİ BAŞARISIZ\033[0m\n\n'
  exit 1
fi
printf '\033[32m  MİMARİ DENETİMİ GEÇTİ\033[0m\n\n'

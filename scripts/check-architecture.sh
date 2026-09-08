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

# --- 3b. Dış süreç sınırı ------------------------------------------------
# Süreç başlatma tek yerde yaşamalı: keşif, doğrulama, zaman aşımı ve ağ
# politikası orada uygulanıyor. Final audit'te yüzeyin iki değil ALTI çağrı
# noktasına yayıldığı ve yarısının ne zaman aşımı ne de ağ politikası taşıdığı
# ölçüldü. Bu kapı, dağılmanın tekrar başlamasını engeller.
#
# İzin verilen istisnalar, her biri gerekçesiyle:
#   * process-bridge          — sınırın kendisi
#   * tests / #[cfg(test)]    — fikstür üreten testler
#   * tools/preflight         — `sandbox-exec`'in KENDİSİNİ sınayan teşhis
#   * open_with_default_application — işletim sistemine devir; beklemek yanlış
SPAWNS=$(grep -rn "Command::new" crates apps tools --include='*.rs' 2>/dev/null \
  | grep -v '/target/' \
  | grep -v 'crates/process-bridge/' \
  | grep -v '/tests/' \
  | grep -vE 'tools/preflight/src/main.rs:[0-9]+: *let (launch|net) =' \
  | grep -vE 'legacy\.rs:[0-9]+: *let ok =' \
  | grep -vE 'legacy_doc\.rs:[0-9]+: *let status =' \
  | grep -vE 'degisikis\.rs:[0-9]+: *let mut command =' \
  | grep -vE 'legacy_doc\.rs:[0-9]+: *let mut command =' || true)
if [ -n "$SPAWNS" ]; then
  bad "sınır dışında süreç başlatma:"
  printf '%s\n' "$SPAWNS" | sed 's/^/      /'
else
  ok "süreç başlatma yalnız process-bridge içinde (belgelenmiş istisnalar hariç)"
fi

# Çekirdekler asla süreç başlatmamalı — istisnasız.
for crate in document-core pdf-core; do
  if [ -d "crates/$crate" ]; then
    N=$(grep -rn "Command::new" "crates/$crate/src" 2>/dev/null | wc -l | tr -d ' ')
    if [ "${N:-0}" -gt 0 ]; then
      bad "$crate süreç başlatıyor — çekirdekler bunu yapamaz"
    else
      ok "$crate: süreç başlatmıyor"
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

# --- 5b. Hiçbir capability geniş yol/kabuk/ağ izni vermemeli -------------
# `opener:allow-open-path` bir yol listesi ister. `"**"` her şeyi açar ve
# uygulamanın en geniş iznidir. Final audit'te ölçüldü: hiçbir kod yolu
# `open_path` veya `open_url` çağırmıyordu; izin bedavaya duruyordu. Geri
# gelmesi kaza olmasın diye kapı bunu fail-closed denetliyor.
BROAD=0
for cap in "$CAPS"/*.json; do
  [ -f "$cap" ] || continue
  if python3 - "$cap" <<'PYEOF'
import json, sys
caps = json.load(open(sys.argv[1]))
bad = []
for p in caps.get("permissions", []):
    if isinstance(p, dict):
        ident = p.get("identifier", "")
        for entry in p.get("allow", []):
            val = entry.get("path") or entry.get("url") or ""
            if val in ("**", "*", "**/*") or val.startswith("**"):
                bad.append(f"{ident}={val}")
    else:
        if p.startswith(("fs:", "shell:", "http:")):
            bad.append(p)
sys.exit(1 if bad else 0)
PYEOF
  then :; else
    bad "geniş izin: $(basename "$cap")"
    BROAD=1
  fi
done
[ "$BROAD" = "0" ] && ok "hiçbir capability geniş yol/kabuk/ağ izni vermiyor"

# --- 5c. Denetle modülü süreç/opener/ağ yüzeyine dokunmamalı -------------
# Capability'ler Tauri'de PENCERE düzeyindedir, modül düzeyinde değil: dört
# modül tek webview'i paylaştığı için izinler ortaktır. Dolayısıyla İkinciGöz'ün
# yalıtımı izinle değil, ÇAĞRI YERİYLE sağlanır — ve burada denetlenir.
IG="$SHELL_SRC/modules/ikincigoz.rs"
if [ -f "$IG" ]; then
  HITS=$(grep -cnIE '\b(Command::new|opener|reveal_item|open_path|open_url|reqwest|TcpStream)\b' "$IG" || true)
  if [ "${HITS:-0}" -gt 0 ]; then
    bad "ikincigoz modülü süreç/opener/ağ yüzeyine dokunuyor ($HITS satır)"
  else
    ok "ikincigoz: süreç, opener ve ağ çağrısı yok"
  fi
fi

# --- 5d. Eski veri yolları yalnız legacy-read katmanında -----------------
# Ayar şeması tek; eski uygulamaların dizin düzenini bilen kod tek yerde
# olmalı. Bir feature modülü eski yolu doğrudan öğrenirse migration katmanı
# atlanabilir hâle gelir ve "asla yazma" sözü kod düzeyinde korunamaz.
LEAK=$(grep -rln 'LEGACY_\|legacy_config_dir\|legacy_webkit_dir' "$SHELL_SRC" 2>/dev/null \
  | grep -v 'paths\.rs' | grep -v 'legacy\.rs' || true)
if [ -n "$LEAK" ]; then
  bad "eski veri yolu legacy katmanı dışında:"
  printf '%s\n' "$LEAK" | sed 's/^/      /'
else
  ok "eski veri yolları yalnız paths.rs ve legacy.rs içinde"
fi

# Migration hiçbir şey silmemeli — kural koddan okunur, belgeden değil.
DEL=$(grep -nE '\b(remove_file|remove_dir_all|remove_dir)\b' "$SHELL_SRC/legacy.rs" 2>/dev/null \
  | grep -v '#\[cfg(test)\]' || true)
DELN=$(awk '/^#\[cfg\(test\)\]/{t=1} !t' "$SHELL_SRC/legacy.rs" 2>/dev/null \
  | grep -cE '\b(remove_file|remove_dir_all|remove_dir)\b' || true)
if [ "${DELN:-0}" -gt 0 ]; then
  bad "migration kodunda silme çağrısı ($DELN)"
else
  ok "migration kodu hiçbir şeyi silmiyor"
fi

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

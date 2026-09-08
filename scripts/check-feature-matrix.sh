#!/usr/bin/env bash
#
# Feature derleme matrisi.
#
# Geri alma sözü bir belge değil, derlenen bir yapılandırmadır. Phase 6'da
# `--no-default-features` derlenmiyordu ve bu, kimse denemediği için fark
# edilmemişti. Burada altı desteklenen şeklin hepsi gerçekten derlenir ve
# desteklenmeyen bir kombinasyonun AÇIKÇA reddedildiği doğrulanır.
set -uo pipefail
cd "$(dirname "$0")/.."

PASS=0; FAIL=0
ok()  { printf '  \033[32m✓\033[0m %s\n' "$*"; PASS=$((PASS+1)); }
bad() { printf '  \033[31m✗\033[0m %s\n' "$*"; FAIL=$((FAIL+1)); }

try() { # try "ad" "feature listesi"
  if cargo check -p belge-shell --no-default-features --features "$2" >/tmp/belge-fm.log 2>&1; then
    ok "$1"
  else
    bad "$1"; grep -E '^error' /tmp/belge-fm.log | head -3 | sed 's/^/      /'
  fi
}

printf '\033[1mFEATURE DERLEME MATRİSİ\033[0m\n'
if cargo check -p belge-shell >/tmp/belge-fm.log 2>&1; then
  ok "dördü birden (varsayılan)"
else
  bad "dördü birden (varsayılan)"; grep -E '^error' /tmp/belge-fm.log | head -3 | sed 's/^/      /'
fi
try "yalnız Dönüştür"    "custom-protocol,feature_tavzih"
try "yalnız Denetle"     "custom-protocol,feature_ikincigoz"
try "yalnız Karşılaştır" "custom-protocol,feature_degisikis"
try "yalnız Düzenle"     "custom-protocol,feature_duzenek"
try "hiçbiri"            "custom-protocol"

# Desteklenmeyen bir kombinasyon SESSİZCE derlenmemeli. Buradaki başarı
# ölçütü derlemenin BAŞARISIZ olmasıdır ve hata açık bir mesaj taşımalıdır.
if cargo check -p belge-shell --no-default-features \
     --features "custom-protocol,feature_tavzih,feature_duzenek" >/tmp/belge-fm.log 2>&1; then
  bad "desteklenmeyen kombinasyon sessizce derlendi — yarım binary riski"
else
  if grep -qc "Desteklenmeyen feature kombinasyonu" /tmp/belge-fm.log; then
    ok "desteklenmeyen kombinasyon açık mesajla reddedildi"
  else
    bad "desteklenmeyen kombinasyon reddedildi ama mesaj açık değil"
  fi
fi

printf '\n  geçen: %d   başarısız: %d\n\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]

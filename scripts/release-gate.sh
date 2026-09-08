#!/usr/bin/env bash
#
# Yüksekbaş Belge sürüm kapısı.
#
# İskelet Tavzih'in release-gate.sh'ından alındı: fail-closed, her kapı ayrı
# raporlanır, atlanan kapı geçmiş sayılmaz. Bu, portföydeki tek olgun sürüm
# kapısıdır ve birleşik uygulama onu miras alır.
#
#   ./scripts/release-gate.sh          tüm kapılar (paketleme dâhil)
#   ./scripts/release-gate.sh --fast   paketleme ve imza denetimini atla
#
# NOT: `grep -q` bir eşleşmede boruyu kapatır; `set -o pipefail` altında
# yukarıdaki komut SIGPIPE ile ölür ve kapı sessizce TERS çevrilir. Bu yüzden
# her yerde `grep -c` ile sayılır.
set -uo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
APP_DIR="$ROOT/apps/belge-shell"

FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

PASS=0; FAIL=0; SKIP=0
FAILED_GATES=()

b()    { printf '\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m %s\n' "$*"; PASS=$((PASS+1)); }
bad()  { printf '  \033[31m✗\033[0m %s\n' "$*"; FAIL=$((FAIL+1)); FAILED_GATES+=("$*"); }
skip() { printf '  \033[33m•\033[0m %s (atlandı)\n' "$*"; SKIP=$((SKIP+1)); }

gate() {
  local name="$1"; shift
  if "$@" >/tmp/belge-gate.log 2>&1; then ok "$name"; else
    bad "$name"; sed 's/^/      /' /tmp/belge-gate.log | tail -25
  fi
}

# Kurulu kullanıcı verisinin parmak izi, hiçbir kapı koşmadan önce. Aşağıdaki son
# kapı bunu yeniden okur ve tek bayt değiştiyse başarısız olur. İki sözleşmeyi
# birden korur: test paketi kurulu ayarlara dokunamaz, migration eski dizinlere
# yazamaz. Tavzih'te birincisinin eksikliği gerçek veri kaybettirdi.
USER_DATA_BEFORE="$(bash "$ROOT/scripts/user-data-fingerprint.sh")"

echo
b "YÜKSEKBAŞ BELGE SÜRÜM KAPISI"
echo "  $(date '+%Y-%m-%d %H:%M:%S')  ·  $ROOT"
echo

b "1. Biçim ve statik denetim"
if command -v cargo >/dev/null; then
  command -v rustfmt >/dev/null \
    && gate "cargo fmt --check" cargo fmt --all -- --check \
    || skip "cargo fmt (rustfmt yok)"
  gate "cargo clippy -D warnings" cargo clippy --workspace --all-targets -- -D warnings
else
  bad "cargo bulunamadı"
fi
gate "tsc --noEmit" bash -c "cd '$APP_DIR' && npx tsc --noEmit"
echo

b "2. Mimari değişmezler"
gate "pdf-core → document-core yönü" bash scripts/check-architecture.sh
# Geri alma yolu bir belge değil, derlenen bir yapılandırmadır. Dört modülün
# tamamı kapalıyken derlenmiyorsa feature bazlı geri alma sözü tutulamaz.
# Altı desteklenen derleme şeklinin hepsi + desteklenmeyen bir kombinasyonun
# açıkça reddedildiği. Tek bir "kapalı derleniyor mu" kontrolü yetmiyor:
# geri alma sözü modül BAŞINA verildi.
gate "feature derleme matrisi (6 şekil)" bash scripts/check-feature-matrix.sh
# Taşınan komutların gövdeleri bağımsız uygulamadakiyle aynı mı? Bağımsız
# komutlar `pub` değil, baseline dondurulmuş; iki katman yan yana çağrılamıyor.
# Bu yüzden eşitlik kaynak düzeyinde kanıtlanır, davranış ise
# tests/duzenek_parity.rs ile ölçülür.
gate "duzenek komut gövdesi eşitliği" python3 scripts/check-command-parity.py
# Dış depo bağımlılığı mandalı. Taze klon → derle → test hedefi, bu sayı
# sıfırlanmadan tutulamaz; artması sessizce olmamalı.
gate "depo dışı bağımlılık mandalı" python3 scripts/check-external-dependencies.py
gate "lisans beyanı tutarlılığı" python3 scripts/check-licensing.py
echo

b "3. Testler"
gate "cargo test --workspace --locked" cargo test --workspace --locked
gate "duzenek standalone↔birleşik davranış matrisi" \
  cargo test -p belge-shell --test duzenek_parity
echo

b "4. Ağ bağımsızlığı"
# Belge işleyen hiçbir katman ağa çıkamaz. Kabuk da bugün çıkmıyor: updater
# henüz yok. Updater eklendiğinde YALNIZ o yüzeyin ağ görmesi beklenir ve bu
# kapı ona göre daraltılır — genişletilmez.
ENGINE_NET=$(grep -rnIE '\b(reqwest|hyper|ureq|curl|TcpStream|UdpSocket|to_socket_addrs)\b' \
     crates apps/belge-shell/src-tauri/src 2>/dev/null | grep -vc '^\s*//' || true)
[ "${ENGINE_NET:-0}" -eq 0 ] && ok "Rust tarafında ağ API'si yok" || {
  bad "Rust tarafında ağ API'si bulundu"
  grep -rnIE '\b(reqwest|hyper|ureq|TcpStream|UdpSocket)\b' crates apps/belge-shell/src-tauri/src | sed 's/^/      /'
}
if command -v cargo >/dev/null; then
  TLS=$(cargo tree -p belge-shell -e normal 2>/dev/null | grep -cE '(^|[ |`-])(reqwest|ureq) ' || true)
  [ "${TLS:-0}" -eq 0 ] && ok "bağımlılık ağacında HTTP istemcisi yok" \
                        || bad "bağımlılık ağacında HTTP istemcisi var"
fi
NETPERM=$(python3 - <<'PY'
import json, glob
bad = []
for f in glob.glob('apps/belge-shell/src-tauri/capabilities/*.json'):
    for p in json.load(open(f)).get('permissions', []):
        name = p if isinstance(p, str) else p.get('identifier', '')
        if name.startswith(('http:', 'shell:', 'fs:')):
            bad.append(name)
print(' '.join(bad))
PY
)
[ -z "$NETPERM" ] && ok "capability'lerde http/shell/fs izni yok" \
                  || bad "beklenmeyen izin: $NETPERM"
FETCH=$(grep -rnIE '\bfetch\(|XMLHttpRequest|WebSocket\(' "$APP_DIR/src" 2>/dev/null | wc -l | tr -d ' ')
[ "${FETCH:-0}" -eq 0 ] && ok "arayüzde doğrudan ağ çağrısı yok" \
                        || bad "arayüzde ağ çağrısı ($FETCH satır)"
echo

b "5. Bağımlılık lisansları"
if command -v cargo >/dev/null; then
  LIC=$(cargo metadata --format-version 1 2>/dev/null | python3 -c "
import json,sys
m=json.load(sys.stdin); bad=[]
for p in m['packages']:
    l=(p.get('license') or '')
    if not l and not p.get('license_file'): bad.append(p['name']+':LİSANSSIZ')
    u=l.upper().replace('LGPL','')
    if 'GPL' in u and ' OR ' not in u: bad.append(p['name']+':'+l)
print(' '.join(bad))
")
  [ -z "$LIC" ] && ok "Rust: GPL/AGPL yok, bilinmeyen lisans yok" || bad "Rust lisans sorunu: $LIC"
fi
NLIC=$(cd "$APP_DIR" && python3 - <<'PY'
import json, glob
bad = []
for pj in glob.glob('node_modules/**/package.json', recursive=True):
    try: d = json.load(open(pj))
    except Exception: continue
    if 'name' not in d or 'version' not in d: continue
    l = d.get('license') or 'BİLİNMEYEN'
    if isinstance(l, dict): l = l.get('type', 'BİLİNMEYEN')
    u = str(l).upper()
    # İkili lisans ("MIT OR GPL-3.0-or-later") kabul edilebilir: izin veren
    # seçenek seçilebilir. Rust kapısı bu kuralı zaten uyguluyordu; npm kapısı
    # uygulamıyordu ve mammoth -> jszip bunu ortaya çıkardı.
    if ('GPL' in u and 'LGPL' not in u and ' OR ' not in u) or u == 'BİLİNMEYEN':
        bad.append(f"{d['name']}:{l}")
print(' '.join(sorted(set(bad))))
PY
)
[ -z "$NLIC" ] && ok "npm: GPL/AGPL yok, bilinmeyen lisans yok" || bad "npm lisans sorunu: $NLIC"
echo

b "6. Bağımlılık kaynağı"
# grep -c eşleşme yokken 1 ile çıkar; `|| echo 0` sayıyı İKİ KEZ üretirdi.
DEP_N=$(grep -cE '^source = "git\+' Cargo.lock 2>/dev/null); DEP_N=${DEP_N:-0}
NPM_N=$(grep -coE '"resolved": "git\+[^"]*"' "$APP_DIR/package-lock.json" 2>/dev/null); NPM_N=${NPM_N:-0}
if [ "${DEP_N:-0}" -eq 0 ] && [ "${NPM_N:-0}" -eq 0 ]; then
  ok "tüm bağımlılıklar paket kayıtlarından (git kaynaklı yok)"
else
  bad "kayıt dışı (git) bağımlılık: cargo=$DEP_N npm=$NPM_N"
fi
echo

b "7. Migration değişmezleri"
# Eski kullanıcı dizinlerinin silinmesi, bu üründe geri alınamaz tek hatadır.
DEL=$(grep -rnIE 'remove_dir_all|remove_file' apps/belge-shell/src-tauri/src/legacy.rs 2>/dev/null \
      | grep -vc '#\[cfg(test)\]\|tests::' || true)
DELREAL=$(awk '/mod tests/{exit} /remove_dir_all|remove_file/{c++} END{print c+0}' \
          apps/belge-shell/src-tauri/src/legacy.rs 2>/dev/null)
[ "${DELREAL:-0}" -eq 0 ] && ok "migration kodu hiçbir şey silmiyor" \
                          || bad "migration kodunda dosya silme ($DELREAL yer)"
gate "migration testleri" cargo test --workspace legacy::
echo

if [ "$FAST" = "1" ]; then
  b "8. Paketleme"; skip "--fast"; echo
else
  b "8. Paketleme ve imza denetimi"
  if [ "${BELGE_ADHOC:-0}" = "1" ]; then
    skip "dağıtım imzası (BELGE_ADHOC=1)"
    gate "yerel paketleme" bash scripts/bundle-macos.sh
  else
    gate "macOS paketleme zinciri (imza + notarization + karantina provası)" bash scripts/bundle-macos.sh
  fi
  echo
fi

if [ "$FAST" = "1" ]; then
  b "9. Taze klon kanıtı"; skip "--fast"; echo
else
  b "9. Taze klon kanıtı"
  # Deponun eski dört depo olmadan klonlanıp derlenip test edilebildiği.
  # Soğuk derleme olduğu için pahalı; bu yüzden yalnız tam koşuda.
  gate "taze klon → kur → derle → test" bash scripts/check-fresh-clone.sh
  echo
fi

b "Kullanıcı verisi"
# Kasten en sonda: yalnız test kapılarını değil, koşunun tamamını gözlemeli.
USER_DATA_AFTER="$(bash "$ROOT/scripts/user-data-fingerprint.sh")"
if [ "$USER_DATA_BEFORE" = "$USER_DATA_AFTER" ]; then
  ok "kurulu kullanıcı verisi değişmedi (birleşik + dört eski dizin)"
else
  bad "kurulu kullanıcı verisi bu koşu sırasında DEĞİŞTİ"
  diff <(printf '%s\n' "$USER_DATA_BEFORE") <(printf '%s\n' "$USER_DATA_AFTER") \
    | sed 's/^/      /' | head -20
  echo "      (bir standalone uygulama açıksa bu meşru olabilir; farkı okuyun)"
fi
echo

b "SONUÇ"
printf "  geçen: %d   başarısız: %d   atlanan: %d\n\n" "$PASS" "$FAIL" "$SKIP"
if [ "$FAIL" -gt 0 ]; then
  printf '\033[31m  SÜRÜM KAPISI BAŞARISIZ\033[0m\n'
  for g in "${FAILED_GATES[@]}"; do printf '    - %s\n' "$g"; done
  echo
  exit 1
fi
printf '\033[32m  SÜRÜM KAPISI GEÇTİ\033[0m\n\n'
echo "  Elle kalan doğrulamalar:"
echo "    1. Taşınan her modülün kendi kabul kayıtları"
echo "    2. VoiceOver ile navigasyon ve modal odak akışı"
echo "    3. Eski ayarların gerçekten taşındığının kullanıcı gözüyle doğrulanması"
echo

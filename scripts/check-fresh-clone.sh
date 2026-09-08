#!/usr/bin/env bash
#
# Taze klon kanıtı.
#
# Bu deponun eski dört depoya ihtiyaç duymadan derlenip test edilebildiğini
# GÖSTERİR — iddia etmez. Klon, eski depoların ERİŞİLEMEDİĞİ bir kökte kurulur.
#
# Eski depolara dokunulmaz: silinmez, taşınmaz, adı değiştirilmez. Erişilemezlik,
# klonun içindeki hiçbir yolun oraya çözülememesiyle sağlanır — çözülen bir yol
# olsaydı `check-external-dependencies.py` zaten kapıyı kapatırdı.
set -uo pipefail
cd "$(dirname "$0")/.."
SRC="$PWD"

PASS=0; FAIL=0
ok()  { printf '  \033[32m✓\033[0m %s\n' "$*"; PASS=$((PASS+1)); }
bad() { printf '  \033[31m✗\033[0m %s\n' "$*"; FAIL=$((FAIL+1)); }

WORK=$(mktemp -d "${TMPDIR:-/tmp}/belge-fresh-XXXXXX")
trap 'rm -rf "$WORK"' EXIT
CLONE="$WORK/Yuksekbas-Belge"

printf '\033[1mTAZE KLON KANITI\033[0m\n'
echo "  klon kökü: $WORK"

if git clone --quiet --no-hardlinks "$SRC" "$CLONE" 2>/dev/null; then
  ok "git clone"
else
  bad "git clone"; exit 1
fi

# 1. Klonun içinde depo dışına çözülen tek bir yol bağımlılığı olmamalı.
if (cd "$CLONE" && python3 scripts/check-external-dependencies.py >/tmp/belge-fc.log 2>&1); then
  ok "dış yol bağımlılığı yok (mandal sıfırda)"
else
  bad "dış yol bağımlılığı bulundu"; sed 's/^/      /' /tmp/belge-fc.log
fi

# 2. Hiçbir manifest, eski depoya çözülen bir YOL bildirmemeli.
#    Yalnız `path = "..."` satırlarına bakılır: yorumlarda eski depo adının
#    geçmesi (nereden geldiğini açıklamak için) bir bağımlılık değildir.
STALE=$(grep -rn 'path *= *"[^"]*\(DuzenEk\|İkinciGöz\|Documents/Tavzih\|Degisik-Is\)' "$CLONE" \
        --include=Cargo.toml --include=package.json 2>/dev/null || true)
if [ -n "$STALE" ]; then
  bad "manifestlerde eski depo yolu:"; printf '%s\n' "$STALE" | sed 's/^/      /'
else
  ok "manifestlerde eski depo yolu yok"
fi

# 3. ÖNCE frontend. Tauri, `generate_context!` sırasında `dist/` dizinini
#    binary'ye gömer; `dist/` git'te tutulmadığı için taze bir klonda Rust
#    derlemesi ondan ÖNCE koşarsa proc macro panikler. Sıra bir tercih değil,
#    zorunluluk — ve taze klon kanıtı tam olarak bunu ortaya çıkarmak içindir.
if [ -d "$SRC/apps/belge-shell/node_modules" ]; then
  cp -R "$SRC/apps/belge-shell/node_modules" "$CLONE/apps/belge-shell/node_modules"
  ok "bağımlılık kurulumu (yerel npm önbelleğinden, ağsız)"
else
  bad "node_modules bulunamadı; ağsız kurulum yapılamıyor"
fi

if (cd "$CLONE/apps/belge-shell" && npx tsc --noEmit >"$WORK/tsc.log" 2>&1); then
  ok "tsc --noEmit"
else
  bad "tsc başarısız"; head -5 "$WORK/tsc.log" | sed 's/^/      /'
fi

if (cd "$CLONE/apps/belge-shell" && npm run build >"$WORK/fe.log" 2>&1); then
  ok "frontend build (dist/ üretildi)"
else
  bad "frontend build başarısız"; grep -iE 'error' "$WORK/fe.log" | head -5 | sed 's/^/      /'
fi

# 4. Rust derlemesi — eski depolar erişilemezken.
#    HOME geçici bir köke alınır: klon, kullanıcının ev dizinindeki eski
#    depolara göreli yolla dahi ulaşamaz. Cargo kayıt önbelleği paylaşılır,
#    yoksa derleme ağa çıkmak zorunda kalırdı.
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
if (cd "$CLONE" && HOME="$WORK/fakehome" cargo build --workspace --offline >"$WORK/build.log" 2>&1); then
  ok "cargo build --workspace (eski depolar erişilemez)"
else
  bad "cargo build başarısız"; grep -E '^error' "$WORK/build.log" | head -5 | sed 's/^/      /'
fi

# 5. Testler.
if (cd "$CLONE" && HOME="$WORK/fakehome" cargo test --workspace --offline >"$WORK/test.log" 2>&1); then
  N=$(grep '^test result' "$WORK/test.log" | awk '{p+=$4} END {print p}')
  ok "cargo test --workspace ($N test geçti)"
else
  bad "cargo test başarısız"
  grep -E '^error|FAILED' "$WORK/test.log" | head -5 | sed 's/^/      /'
fi

printf '\n  geçen: %d   başarısız: %d\n\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]

#!/usr/bin/env bash
#
# Kurulu kullanıcı verisinin salt-okunur parmak izi.
#
# Birleşik uygulamanın kendi dizini ve dört eski uygulamanın veri dizinleri.
# Her dosya için sha256, bayt boyutu, değişiklik zamanı ve yol basar; dizin yoksa
# YOK yazar. Hiçbir şey yazmaz, oluşturmaz, silmez — tek işi *başkasının*
# yazmadığını kanıtlamaktır.
#
# İki sözleşmeyi birden korur:
#   1. Test paketi kurulu ayarlara dokunamaz  (Tavzih'te bu kusur veri kaybettirdi)
#   2. Migration eski dizinlere asla yazmaz    (yalnız okur, asla silmez)
set -uo pipefail

SUPPORT="$HOME/Library/Application Support"
WEBKIT="$HOME/Library/WebKit"
case "$(uname -s)" in
  Darwin) : ;;
  MINGW*|MSYS*|CYGWIN*) SUPPORT="${APPDATA:-$HOME/AppData/Roaming}"; WEBKIT="$SUPPORT" ;;
  *) SUPPORT="${XDG_CONFIG_HOME:-$HOME/.config}"; WEBKIT="$SUPPORT" ;;
esac

dirs=(
  "$SUPPORT/tr.yuksekbas.golgedosya"
  "$SUPPORT/tr.yuksekbas.belge"
  "$SUPPORT/tr.yuksekbas.tavzih"
  "$SUPPORT/tr.yuksekbas.ikincigoz"
  "$SUPPORT/tr.yuksekbas.duzenek"
  "$SUPPORT/tr.degisikis.desktop"
  "$WEBKIT/tr.yuksekbas.duzenek"
  "$WEBKIT/tr.degisikis.desktop"
)

# WebKit dizinlerinin çoğu WebKit'in kendi defteridir (ResourceLoadStatistics,
# EnhancedSecurity, MediaKeys, -wal/-shm) ve uygulama çalışmıyorken bile arka
# planda değişir. Bunları parmak izine katmak kapıyı gürültüye boğar. Yalnız
# uygulamanın gerçekten sahip olduğu veriye bakılır: yapılandırma JSON'ları ve
# localStorage veritabanının kendisi.
keep() {
  case "$1" in
    *-wal|*-shm) return 1 ;;
    */ResourceLoadStatistics/*|*/EnhancedSecurity/*|*/MediaKeys/*) return 1 ;;
    *) return 0 ;;
  esac
}

for d in "${dirs[@]}"; do
  if [ ! -d "$d" ]; then
    printf 'YOK  %s\n' "$d"
    continue
  fi
  while IFS= read -r -d '' f; do
    keep "$f" || continue
    printf '%s  %s  %s  %s\n' \
      "$(shasum -a 256 "$f" 2>/dev/null | awk '{print $1}')" \
      "$(stat -f '%z' "$f" 2>/dev/null)" \
      "$(stat -f '%m' "$f" 2>/dev/null)" \
      "$f"
  done < <(find "$d" -type f -print0 2>/dev/null | LC_ALL=C sort -z)
done

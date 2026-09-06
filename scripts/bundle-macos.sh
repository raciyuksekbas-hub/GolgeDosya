#!/bin/sh
#
# macOS paketleme zinciri.
#
# İki bağımsız uygulamanın hattından, her birinin EN GÜÇLÜ kontrolü alınarak
# birleştirildi:
#
#   Tavzih (scripts/bundle-macos.sh):
#     * `tauri build` tek başına bundle'ı MÜHÜRLEMEZ; geriye yalnız Rust
#       linker'ının ad-hoc imzası kalır (flags 0x20002). Bu imza "sealed
#       resources var" der ama _CodeSignature/CodeResources hiç yazılmaz;
#       karantinalı kopya "hasarlı" diye açılmaz. Tavzih v1.0.0 tam olarak
#       bu durumda yayımlandı. Çözüm: bitmiş bundle'ı kendimiz imzalamak ve
#       ondan sonra ona bir daha dokunmamak — DMG ayrı bir staging dizininde
#       kurulur.
#     * notarization sonucu ÇIKIŞ KODUNDAN değil METİNDEN okunur:
#       `notarytool --wait` Invalid sonucunda da 0 döndürebiliyor.
#
#   Değişikİş (scripts/release-macos.sh):
#     * genişletilmiş öznitelik temizliği (com.apple.FinderInfo codesign'ı
#       reddettirir; iCloud'da tutulan bir projede gerçek bir risk).
#     * KARANTİNA GİDİŞ-DÖNÜŞÜ: DMG'ye com.apple.quarantine yazılır, mount
#       edilir ve İÇİNDEKİ .app `spctl --assess --type exec` ile denetlenir.
#       Kullanıcının indirdiğinde yaşayacağı şeyin tek gerçek provası budur.
#
# BELGE_ADHOC=1 yerel, imzasız derleme yapar. Yapısal olarak geçerli ama
# karantinalı kopyada Gatekeeper yine durdurur — hızlı yerel iş için, asla sürüm için.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
APP_DIR="$ROOT/apps/belge-shell"
TARGET_DIR="$ROOT/target/release/bundle"
DMG_DIR="$TARGET_DIR/dmg"

PRODUCT_NAME=$(node -p "require('$APP_DIR/src-tauri/tauri.conf.json').productName")
VERSION=$(node -p "require('$APP_DIR/src-tauri/tauri.conf.json').version")
BUNDLE_ID=$(node -p "require('$APP_DIR/src-tauri/tauri.conf.json').identifier")
case $(uname -m) in
  arm64) ARCH=aarch64 ;;
  *) ARCH=$(uname -m) ;;
esac
APP="$TARGET_DIR/macos/$PRODUCT_NAME.app"
DMG="$DMG_DIR/${PRODUCT_NAME}_${VERSION}_${ARCH}.dmg"

STAGING_DIR=$(mktemp -d "${TMPDIR:-/tmp}/belge-dmg.XXXXXX")
WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/belge-notary.XXXXXX")
MOUNTPOINT=$(mktemp -d "${TMPDIR:-/tmp}/belge-mount.XXXXXX")
cleanup() {
  hdiutil detach "$MOUNTPOINT" -quiet 2>/dev/null || true
  rm -rf "$STAGING_DIR" "$WORK_DIR" "$MOUNTPOINT"
}
trap cleanup EXIT HUP INT TERM

ADHOC=${BELGE_ADHOC:-0}
IDENTITY='-'
NOTARY_PROFILE=''

if [ "$ADHOC" != "1" ]; then
  IDENTITY=${BELGE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning \
    | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)}
  if [ -z "$IDENTITY" ]; then
    echo "Anahtarlıkta Developer ID Application sertifikası yok." >&2
    echo "  security find-identity -v -p codesigning   # birini listelemeli" >&2
    echo "  BELGE_ADHOC=1 $0                           # imzasız yerel derleme" >&2
    exit 1
  fi
  # notarytool kimlik bilgisini anahtarlıkta tutar; hiçbir betik parolayı görmez.
  for candidate in ${BELGE_NOTARY_PROFILE:-} belge yuksekbas tavzih; do
    [ -n "$candidate" ] || continue
    if xcrun notarytool history --keychain-profile "$candidate" >/dev/null 2>&1; then
      NOTARY_PROFILE="$candidate"
      break
    fi
  done
  if [ -z "$NOTARY_PROFILE" ]; then
    echo "Kullanılabilir notarytool anahtarlık profili yok." >&2
    echo "  xcrun notarytool store-credentials belge --apple-id <id> --team-id <team>" >&2
    exit 1
  fi
fi

# `notarytool --wait` Invalid sonucunda da 0 döndürebiliyor. Karar METİNDEN okunur.
notarize() {
  _rc=0
  _out=$(xcrun notarytool submit "$1" --keychain-profile "$NOTARY_PROFILE" --wait 2>&1) || _rc=$?
  printf '%s\n' "$_out"
  if ! printf '%s' "$_out" | grep -q 'status: Accepted'; then
    echo "Notarization KABUL EDİLMEDİ (çıkış kodu $_rc)." >&2
    _id=$(printf '%s' "$_out" | sed -n 's/.*id: \([0-9a-f-]\{36\}\).*/\1/p' | head -1)
    [ -n "$_id" ] && xcrun notarytool log "$_id" --keychain-profile "$NOTARY_PROFILE" >&2 2>&1 || true
    exit 1
  fi
}

echo "==> Derleme"
( cd "$APP_DIR" && npm run tauri build -- --bundles app )

[ -d "$APP" ] || { echo "Bundle üretilmedi: $APP" >&2; exit 1; }

echo "==> Genişletilmiş öznitelik temizliği"
# com.apple.FinderInfo codesign'ı reddettirir. iCloud'da tutulan bir kaynak
# ağacında bu öznitelik kendiliğinden oluşur.
xattr -cr "$APP"

echo "==> İmzalama"
if [ "$ADHOC" = "1" ]; then
  codesign --force --deep --sign - "$APP"
else
  # Entitlement yok. Uygulama yalnız kullanıcının seçtiği dosyayı okur ve
  # sonucu kullanıcının seçtiği yere yazar; notarization'ın istediği tek şey
  # --options runtime ve --timestamp. Boş yere sandbox genişletmiyoruz.
  #
  # NOT (Phase 6): DüzenEk taşındığında bu duruş yeniden sınanmalıdır —
  # sandbox-exec, soffice, sips ve CoreGraphics hardened runtime altında hiç
  # denenmedi. Bkz. docs/MIGRATION.md "Açık teknik sorular".
  codesign --force --deep --options runtime --timestamp --sign "$IDENTITY" "$APP"
fi

codesign --verify --deep --strict --verbose=4 "$APP"
[ -f "$APP/Contents/_CodeSignature/CodeResources" ] || {
  echo "_CodeSignature/CodeResources yok — bundle mühürsüz." >&2; exit 1; }

if [ "$ADHOC" != "1" ]; then
  SIGINFO=$(codesign -dv --verbose=4 "$APP" 2>&1 || true)
  printf '%s\n' "$SIGINFO" | grep -qE 'flags=.*runtime' || {
    echo "Hardened Runtime bayrağı yok." >&2; exit 1; }
  printf '%s\n' "$SIGINFO" | grep -q 'Timestamp=' || {
    echo "Güvenli zaman damgası yok." >&2; exit 1; }
  printf '%s\n' "$SIGINFO" | grep -q 'linker-signed' && {
    echo "Yalnız linker-signed imza — bundle imzalanmamış." >&2; exit 1; }

  echo "==> .app notarization"
  notarize "$APP"
  xcrun stapler staple "$APP"
  xcrun stapler validate "$APP"
fi

echo "==> DMG"
# Mühürlenmiş .app'e bir daha dokunulmaz: DMG ayrı bir staging'den kurulur.
mkdir -p "$DMG_DIR"
cp -R "$APP" "$STAGING_DIR/"
ln -s /Applications "$STAGING_DIR/Applications"
codesign --verify --deep --strict "$STAGING_DIR/$PRODUCT_NAME.app"
[ "$ADHOC" = "1" ] || xcrun stapler validate "$STAGING_DIR/$PRODUCT_NAME.app"

rm -f "$DMG"
hdiutil create -volname "$PRODUCT_NAME" -srcfolder "$STAGING_DIR" -ov -format UDZO "$DMG"
hdiutil verify "$DMG"

if [ "$ADHOC" != "1" ]; then
  codesign --force --timestamp --sign "$IDENTITY" "$DMG"
  codesign --verify --strict --verbose=2 "$DMG"
  echo "==> DMG notarization"
  notarize "$DMG"
  xcrun stapler staple "$DMG"
  xcrun stapler validate "$DMG"

  echo "==> Karantina gidiş-dönüşü (indirilen kopyanın provası)"
  # Bu, hattın en değerli kontrolü: kullanıcı DMG'yi indirdiğinde Gatekeeper
  # ne diyecekse, burada onu görüyoruz.
  QUARANTINED="$WORK_DIR/$(basename "$DMG")"
  cp "$DMG" "$QUARANTINED"
  xattr -w com.apple.quarantine "0081;00000000;Safari;" "$QUARANTINED"
  hdiutil attach "$QUARANTINED" -nobrowse -readonly -mountpoint "$MOUNTPOINT"
  MOUNTED_APP="$MOUNTPOINT/$PRODUCT_NAME.app"
  codesign --verify --deep --strict --verbose=4 "$MOUNTED_APP"
  xcrun stapler validate "$MOUNTED_APP"
  spctl --assess --type exec -vv "$MOUNTED_APP"
  hdiutil detach "$MOUNTPOINT" -quiet
  spctl --assess --type open --context context:primary-signature -vv "$DMG"

  ID=$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$APP/Contents/Info.plist")
  [ "$ID" = "$BUNDLE_ID" ] || { echo "Paket kimliği yanlış: $ID" >&2; exit 1; }
fi

echo
echo "Tamamlandı:"
echo "  $APP"
echo "  $DMG"

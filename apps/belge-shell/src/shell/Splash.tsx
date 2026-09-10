import { AppMark } from "./icons";

/**
 * Açılış ekranı.
 *
 * Sahte bir bekleme değil: kabuk, açılış IPC'si (feature listesi, ayarlar,
 * eski ayar taşıması) dönene kadar zaten hiçbir şey çizemiyordu ve boş bir
 * pencere gösteriyordu. Burada o an ürünün adını taşıyor. İş biter bitmez
 * kaybolur; zamanlayıcı yok, animasyon yok.
 *
 * Tek bir şey söyler: ürünün adı. Slogan yok, ilerleme çubuğu yok. Sürüm
 * derleme zamanından gelir (`__APP_VERSION__`), IPC'yi beklemez.
 */
export function Splash() {
  return (
    <div className="splash" role="status" aria-label="GölgeDosya açılıyor">
      <AppMark className="splash-mark" />
      <p className="splash-name">GölgeDosya</p>
      <p className="splash-version">Sürüm {__APP_VERSION__}</p>
    </div>
  );
}

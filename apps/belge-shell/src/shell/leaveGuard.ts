/**
 * Kaybettiren eylemler sözleşmesi (§54).
 *
 * Çalışma durumu bellekte yaşar. Yalnız Ekler'in düzeni kip değişince korunur
 * (kabukta durur); Düzenle'nin kaydedilmemiş sayfa sırası ve döndürmeleri,
 * çalışma alanı söküldüğü anda silinir. Bu dosya tek bir soruyu cevaplar:
 * bu eylem, kaydedilmemiş bir çalışmayı SESSİZCE siler mi? Evetse kabuk önce
 * sorar ("Mevcut çalışma temizlenecek. Devam edilsin mi?"); hayırsa eylem
 * hemen olur. Kayıpsız eylem hiçbir zaman sorulmaz.
 *
 * Yenile (F5 / Ctrl+R / tarayıcı menüsü) burada yok: o yüzey tamamen
 * kaldırıldı (browserSurface).
 */
export type LeaveAction =
  /** Başka bir kipe geçmek: çalışma alanı sökülür. */
  | "navigate"
  /** Bardaki "Kapat": belge bırakılır. */
  | "close"
  /** Ürün işaretine tıklamak: kipin başlangıç yüzeyine dönmek. */
  | "home"
  /** Açık belgenin yerine başka bir belge açmak. */
  | "replace"
  /** Pencereyi kapatmak: bütün oturum gider. */
  | "window";

export interface WorkState {
  /** Etkin kip. */
  mode: string | null;
  /** Kip başına: kaydedilmemiş çalışma var mı? (çalışma alanı bildirir) */
  dirty: Partial<Record<string, boolean>>;
  /** Ekler'in hazırlanmamış düzeni (kabukta yaşar; yalnız pencere kapanınca gider). */
  annexUnsaved: boolean;
}

export function leaveLoses(action: LeaveAction, state: WorkState, target?: string): boolean {
  if (action === "window") return state.annexUnsaved || Object.values(state.dirty).some(Boolean);
  if (!state.mode) return false;
  // Ekler'in oturumu kabuktadır: kip değiştirmek ya da Kapat ona dokunmaz.
  if (state.mode === "ekler") return false;
  if (action === "navigate" && target === state.mode) return false;
  return Boolean(state.dirty[state.mode]);
}

/** Onay penceresinin metni. Ne kaybolacağını söyler; ne kalacağını da. */
export function leaveCopy(action: LeaveAction): { title: string; body: string; confirm: string } {
  if (action === "window") {
    return {
      title: "Kaydedilmemiş çalışma var",
      body: "Pencere kapanınca kaydedilmemiş sayfa düzeni ya da hazırlanmamış ek düzeni silinir. Kaydettiğiniz dosyalar ve kaynak belgeleriniz diskte kalır. Kapatılsın mı?",
      confirm: "Kapat",
    };
  }
  return {
    title: "Mevcut çalışma temizlenecek",
    body: "Sayfa sırası ve döndürme değişiklikleriniz henüz bir kopyaya kaydedilmedi. Kaynak belgeniz değişmez. Devam edilsin mi?",
    confirm: action === "navigate" ? "Devam Et" : "Temizle",
  };
}

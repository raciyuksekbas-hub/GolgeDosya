/**
 * Göreli zaman — "az önce", "12 dk önce", "dün", "3 gün önce", "12 Eyl".
 *
 * Girdi Unix SANİYEDİR: kabuk son kullanılanları `remember_documents` ile
 * Rust'tan alır ve Rust `SystemTime::as_secs()` yazar. Milisaniye varsayan
 * eski sürüm gerçek uygulamada her satırı "1 Oca" gösteriyordu; tarayıcı
 * taklidi ms gönderdiği için kusur orada görünmüyordu. Dönüşüm burada, sunum
 * sınırında yapılır — Rust sözleşmesi değişmez.
 */
export function relativeTime(openedAtSeconds: number, now = Date.now()): string {
  const diff = now - openedAtSeconds * 1000;
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diff < minute) return "az önce";
  if (diff < hour) return `${Math.round(diff / minute)} dk önce`;
  if (diff < day) return `${Math.round(diff / hour)} sa önce`;
  if (diff < 2 * day) return "dün";
  if (diff < 7 * day) return `${Math.floor(diff / day)} gün önce`;
  return new Date(openedAtSeconds * 1000).toLocaleDateString("tr-TR", { day: "numeric", month: "short" });
}

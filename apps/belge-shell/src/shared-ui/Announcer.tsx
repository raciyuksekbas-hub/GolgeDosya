/**
 * Ekran okuyucuya durum bildiren tek canlı bölge.
 *
 * Dört uygulamanın dördü de kendi duyurucusunu yazmıştı. Tek bir tanesi
 * yeterli: sayfada birden fazla `aria-live` bölgesi olması, duyuruların
 * birbirini bastırmasına yol açar.
 */
import { useEffect, useRef, useState } from "react";

let publish: ((message: string) => void) | null = null;

/** Nazik duyuru. Kullanıcının o an yaptığı işi kesmez. */
export function announce(message: string) {
  publish?.(message);
}

export function Announcer() {
  const [message, setMessage] = useState("");
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    publish = (next: string) => {
      // Aynı metni art arda yazmak bazı ekran okuyucularda hiç okunmamasına
      // yol açar; önce boşaltıp sonra yazıyoruz.
      setMessage("");
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setMessage(next), 50);
    };
    return () => {
      publish = null;
      window.clearTimeout(timer.current);
    };
  }, []);

  return (
    <div aria-live="polite" aria-atomic="true" className="sr-only" data-testid="announcer">
      {message}
    </div>
  );
}

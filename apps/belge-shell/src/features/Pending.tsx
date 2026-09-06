import type { FeatureState } from "../shell/types";

const NOTES: Record<FeatureState["key"], { source: string; phase: string; note: string }> = {
  duzenek: {
    source: "DüzenEk",
    phase: "Phase 6",
    note: "PDF sayfa işlemleri, önizleme, zorunlu logo ve Office→PDF köprüsü. Dörtlünün son taşınan modülü: bağımsız uygulamada kabul süreci sürüyor ve taşımadan önce dondurulmuş bir davranış temeli gerekiyor.",
  },
  tavzih: {
    source: "Tavzih",
    phase: "Phase 3",
    note: "DOCX ↔ UDF dönüşümü. İlk taşınacak modül: belge çekirdeğinin doğruluk kanıtı (57 korpus testi) burada.",
  },
  degisikis: {
    source: "Değişikİş",
    phase: "Phase 5",
    note: "Belge karşılaştırma. Diff motoru TypeScript'te kalır; Rust'a çevrilmez.",
  },
  ikincigoz: {
    source: "İkinciGöz",
    phase: "Phase 4",
    note: "Belge denetimi. Mevcut parser ve byte düzeyinde write-back sözleşmesi korunarak taşınır.",
  },
};

/**
 * Henüz taşınmamış bir modülün yeri.
 *
 * Bu bir "yakında" ekranı değil, bir migration durum kaydıdır: hangi kod
 * tabanından geleceği, hangi fazda ve neden o sırada olduğu yazılıdır.
 */
export function Pending({ feature }: { feature: FeatureState }) {
  const note = NOTES[feature.key];
  return (
    <section className="placeholder" aria-labelledby="pending-baslik">
      <h1 id="pending-baslik">{feature.label}</h1>
      <p>Bu bölüm birleşik uygulamaya henüz taşınmadı.</p>
      <dl>
        <dt>Kaynak uygulama</dt>
        <dd>{note.source} — bağımsız uygulama çalışmaya devam ediyor.</dd>
        <dt>Planlanan aşama</dt>
        <dd>{note.phase}</dd>
        <dt>Kapsam</dt>
        <dd>{note.note}</dd>
        <dt>Feature anahtarı</dt>
        <dd><code>{feature.key}</code> · <code>BELGE_DISABLE_{feature.key.toUpperCase()}=1</code> ile kapatılabilir</dd>
      </dl>
    </section>
  );
}

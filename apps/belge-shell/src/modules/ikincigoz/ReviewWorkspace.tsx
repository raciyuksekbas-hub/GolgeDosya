import { useCallback, useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { AnalysisResult, Finding, Fix, Severity } from "./types";
import { announce } from "../../shared-ui/Announcer";
import { logFailure, safeMessage } from "../../shared-ui/failure";
import { Button, EmptyState, Status } from "../../shared-ui/primitives";
import { InspectorPanel, InspectorSection, ToolbarActions, ToolbarStatus } from "../../shell/chrome";
import { startFolder } from "../../shared-ui/savePlace";
import { correctedSaveOptions } from "./saveTarget";

/**
 * Severity'nin metin karşılığı.
 *
 * Bulgu ciddiyeti ASLA yalnız renkle verilmez: her satır bu etiketi ve kendi
 * şeklini taşır. Renk körlüğü ve yüksek kontrast modunda ayrım kaybolmamalı.
 */
export const SEVERITY: Record<Severity, { label: string; mark: string }> = {
  error: { label: "Kesin hata", mark: "●" },
  warning: { label: "Uyarı", mark: "▲" },
  review: { label: "İncele", mark: "○" },
};

function summary(r: AnalysisResult): string {
  return [
    `${r.errorCount} kesin hata`,
    `${r.warningCount} uyarı`,
    `${r.reviewCount} incele`,
  ].join(" · ");
}

/**
 * Düzeltmenin görünür yazımı.
 *
 * Boşluk düzeltmelerinde eski ve yeni değer yalnız boşluktan ibaret; HTML
 * boşlukları daralttığı için `<code>` kutuları bomboş çiziliyordu ve kullanıcı
 * "Önerilen düzeltme:  →  " diye bir şey okuyordu (gerçek pencerede görüldü).
 * Boşluklar görünür bir işaretle yazılır; metin değişmez.
 *
 * İşaret ␣ (U+2423), "·" değil: nokta uygulamanın her yerinde ayırıcıdır
 * ("3 kesin hata · 2 uyarı") ve art arda gelince üç nokta gibi okunuyordu.
 * Bölünmez boşluk ve sıfır genişlikliler alıntıdaki gibi "[U+XXXX]" yazılır;
 * yoksa "bölünmez boşluk → tek boşluk" düzeltmesi boş bir kutu gösteriyordu.
 * Bu yalnız gösterimdir; ekran okuyucu `spokenText` karşılığını duyar ve
 * belgeye giden düzeltme motorun kendi `Fix` nesnesidir.
 */
export function visibleText(text: string): string {
  if (text.length === 0) return "∅";
  return text
    .replace(/ /g, "␣")
    .replace(/\t/g, "⇥")
    .replace(new RegExp(INVISIBLE.source, "gu"), (c) => `[${codePoint(c)}]`);
}

/** Düzeltme değerinin sözlü karşılığı: "2 boşluk", boşsa "" — metinse kendisi. */
function spokenText(text: string): string {
  return visibleRange(text)?.spoken ?? text;
}

/** Düzeltme satırının ekran okuyucu cümlesi: "2 boşluk yerine 1 boşluk". */
export function spokenFix(fix: Pick<Fix, "original" | "replacement">): string {
  const from = spokenText(fix.original);
  const to = spokenText(fix.replacement);
  if (!to) return `${from} silinir`;
  if (!from) return `${to} eklenir`;
  return `${from} yerine ${to}`;
}

/** Görünmeyen ama belgede duran karakterler: bölünmez boşluk, sıfır genişlikliler, yumuşak tire. */
const INVISIBLE = /[\u00A0\u00AD\u200B-\u200D\u2060\uFEFF]/u;

const codePoint = (c: string) => `U+${c.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0")}`;

/**
 * İşaretli aralık yalnız boşluk ya da görünmeyen karakterse onu GÖRÜNÜR yapar.
 *
 * Saha (madde 35): "Denetle 'çok boşluk'u vurgulamıyor." Motor aralığı doğru
 * veriyordu; ama boşluklar HTML'de tek boşluğa daralıyordu: iki ve yedi
 * boşluk aynı 3,6 px'lik şeride, sıfır genişlikli karakter 0 px'e iniyordu —
 * vurgu vardı ama görünmüyordu. Ekran okuyucu da bir şey duymuyordu.
 *
 * Görünür biçim düzeltme satırıyla aynı dil: boşluk "␣", sekme "⇥". Bölünmez
 * boşluk ve sıfır genişlikliler "[U+XXXX]" — Windows yazı tiplerinde karşılığı
 * olmayabilecek simgeler yerine her yazı tipinde çizilen ASCII. `spoken`,
 * ekran okuyucunun duyacağı karşılıktır.
 *
 * İşaret METİN DEĞİLDİR (v0.3.2). Eskiden "·" boşlukların yerine DOM'a
 * yazılıyordu; paragraf kopyalanınca boşluklar kayboluyor, yerine ekran
 * okuyucu metni geliyordu ("Davacı7 boşlukvekili"). Artık her karakter DOM'da
 * aynen kalır; `chars[i].label` yalnız CSS `::before` ile çizilir (bkz.
 * MarkedSlice ve shell.css). `shown`, gözün gördüğü dizinin kendisidir.
 */
export type VisibleChar = {
  ch: string;
  /** "other": /\s/ ile eşleşen öteki boşluklar (ör. em boşluk); işaretsiz kalır. */
  kind: "space" | "tab" | "invisible" | "other";
  label: string;
};

export function visibleRange(
  slice: string,
): { chars: VisibleChar[]; shown: string; spoken: string } | null {
  const raw = [...slice];
  if (raw.length === 0 || !raw.every((c) => /\s/u.test(c) || INVISIBLE.test(c))) return null;
  const chars: VisibleChar[] = raw.map((ch) =>
    ch === " "
      ? { ch, kind: "space", label: "␣" }
      : ch === "\t"
        ? { ch, kind: "tab", label: "⇥" }
        : INVISIBLE.test(ch)
          ? { ch, kind: "invisible", label: `[${codePoint(ch)}]` }
          : { ch, kind: "other", label: ch },
  );
  const spaces = raw.filter((c) => c === " ").length;
  const parts = [
    spaces ? `${spaces} boşluk` : "",
    raw.some((c) => c === "\t") ? "sekme" : "",
    raw.some((c) => c === "\u00A0") ? "bölünmez boşluk" : "",
    ...[...new Set(raw.filter((c) => INVISIBLE.test(c) && c !== "\u00A0"))].map(
      (c) => `görünmeyen karakter (${codePoint(c)})`,
    ),
  ].filter(Boolean);
  return { chars, shown: chars.map((c) => c.label).join(""), spoken: parts.join(", ") };
}

/**
 * Bulgunun işaretli aralığı.
 *
 * Motor `SourceLocation`'ı snake_case seri hâle getiriyor (`char_start`),
 * arayüz tipi ise camelCase yazıyor. Bu yüzden ofsetler pratikte `undefined`
 * geliyordu ve `slice(0, undefined)` üç kez tüm paragrafı döndürüyordu:
 * paketlenmiş uygulamada alıntı aynı cümleyi üç kez yazıyordu. Burada iki
 * yazım da okunur ve yalnız gerçek sayı çifti işaretlenir; ofset yoksa
 * paragraf işaretsiz gösterilir. Motor sözleşmesine dokunulmadı.
 */
/**
 * Kullanıcıya gösterilen paragraf numarası — 1 TABANLI.
 *
 * `block_id` motorda `p{block_index}`tir ve `block_index` SIFIR tabanlıdır.
 * Olduğu gibi yazılınca belgenin İLK paragrafı "0. paragraf" görünüyordu;
 * avukat belgesinde o numarayı arayınca bulamaz. Motorun kendi CLI örneği de
 * insana `block_index + 1` yazdırır — ekran da aynı tabanı kullanmalı.
 *
 * Alan adı iki yazımda da okunur: motor snake_case gönderir (bkz. markedRange).
 */
export function paragraphNumber(f: Pick<Finding, "block_id" | "location">): number {
  const raw = f.location as unknown as Record<string, unknown>;
  const index = typeof raw.blockIndex === "number" ? raw.blockIndex : raw.block_index;
  if (typeof index === "number") return index + 1;
  const fromId = Number.parseInt(f.block_id.replace(/^p/, ""), 10);
  return Number.isFinite(fromId) ? fromId + 1 : 1;
}

function markedRange(location: Finding["location"]): [number, number] | null {
  const raw = location as unknown as Record<string, unknown>;
  const from = typeof raw.charStart === "number" ? raw.charStart : raw.char_start;
  const to = typeof raw.charEnd === "number" ? raw.charEnd : raw.char_end;
  if (typeof from !== "number" || typeof to !== "number") return null;
  return to > from ? [from, to] : null;
}

/**
 * Barın eylemi ve sağdaki denetim özeti.
 *
 * Yalnız bulgu varken vardır. Sıfır bulguda panel üç kez "0" yazan bir sütuna
 * dönüyor, boş-başarı durumunu ikiye bölüyordu; kip o durumda üçüncü kolonu
 * hiç açmaz. Sayı ve ciddiyet anlamları değişmedi — yalnız çizildikleri koşul.
 */
export function ReviewChrome({
  result,
  fixable,
  selected,
  onApply,
  onSelectAll,
  onClearSelection,
}: {
  result: AnalysisResult;
  fixable: Finding[];
  selected: Set<string>;
  onApply: () => void;
  onSelectAll: () => void;
  onClearSelection: () => void;
}) {
  if (result.findings.length === 0) return null;
  const doc = result.document;

  return (
    <>
      {fixable.length > 0 ? (
        <ToolbarActions>
          <Button variant="primary" onClick={onApply} disabled={selected.size === 0}>
            Kopyaya Uygula
          </Button>
        </ToolbarActions>
      ) : null}

      <InspectorPanel title="Denetim özeti">
        <div className="kv">
          <span className="kv-key">{SEVERITY.error.label}</span>
          <span className="kv-value">{result.errorCount}</span>
        </div>
        <div className="kv">
          <span className="kv-key">{SEVERITY.warning.label}</span>
          <span className="kv-value">{result.warningCount}</span>
        </div>
        <div className="kv">
          <span className="kv-key">{SEVERITY.review.label}</span>
          <span className="kv-value">{result.reviewCount}</span>
        </div>
        <p className="tool-hint">
          {doc.blockCount} paragraf · {doc.wordCount} kelime
          {result.profileName ? ` · ${result.profileName} profili` : ""}
        </p>
        {result.truncatedRules.length > 0 ? (
          <p className="tool-hint">
            {result.truncatedRules
              .map((t) => `${t.total} bulgunun ${t.shown} tanesi gösteriliyor`)
              .join(" · ")}
          </p>
        ) : null}

        {fixable.length > 0 ? (
          <InspectorSection title="Düzeltmeler">
            <p className="fix-count">
              {selected.size} / {fixable.length} seçili
            </p>
            <div className="row">
              <Button className="btn-sm" onClick={onSelectAll} disabled={selected.size === fixable.length}>
                Tümünü Seç
              </Button>
              <Button
                className="btn-sm"
                variant="quiet"
                onClick={onClearSelection}
                disabled={selected.size === 0}
              >
                Seçimi Temizle
              </Button>
            </div>
            <p className="tool-hint">
              Düzeltmeler yeni bir kopyaya yazılır. Kaynak belgeniz değiştirilmez.
            </p>
          </InspectorSection>
        ) : null}
      </InspectorPanel>
    </>
  );
}

/**
 * Temiz belge: gösteri değil, sakin bir kapanış.
 *
 * Kutu, kart, ikon ya da başarı rengi yok. İncelemenin gerçekten yapıldığını
 * gösteren belge künyesi tek muted satır olarak burada durur; yalnız bu iki
 * sayı için sağda bir panel açmak yersizdi.
 */
export function ReviewClear({ doc }: { doc: AnalysisResult["document"] }) {
  return (
    <EmptyState
      title="Bulgu bulunmadı"
      note="Bu belge tanımlı kuralların hiçbirine takılmadı."
      meta={`${doc.blockCount} paragraf · ${doc.wordCount} kelime incelendi`}
    />
  );
}

/**
 * Bulgunun kimliği — akordeon ve düzeltme seçimi bunu paylaşır.
 *
 * Motor `SourceLocation`'ı snake_case gönderiyor (`char_start`), arayüz tipi
 * camelCase. `f.location.charStart` bu yüzden çalışma zamanında `undefined`
 * geliyor ve aynı kuralın aynı paragraftaki BÜTÜN bulguları tek bir
 * `kural:blok:-1` anahtarına çöküyordu: tek bir onay kutusu işaretlenince
 * kullanıcının SEÇMEDİĞİ düzeltmeler de uygulanıyordu (belgede istenmeyen
 * değişiklik). Anahtar iki yazımı da okur ve aralık + mesajla benzersizleşir;
 * motor sözleşmesine dokunulmaz. Ayraç olarak `\u0000` kullanılır ki mesajdaki
 * `:` iki bulguyu birbirine karıştırmasın.
 */
export const findingKey = (f: Finding) => {
  const raw = f.location as unknown as Record<string, unknown>;
  const num = (camel: string, snake: string) => {
    const c = raw[camel];
    if (typeof c === "number") return c;
    const s = raw[snake];
    return typeof s === "number" ? s : -1;
  };
  const start = num("charStart", "char_start");
  const end = num("charEnd", "char_end");
  return [f.rule_id, f.block_id, start, end, f.message].join("\u0000");
};

/**
 * Bulgu listesi — satır içi akordeon.
 *
 * Kanıt bulgunun yanında durur: paragrafın tamamı, işaretli aralık vurgulu.
 * Saf sunum; durumu ve seçimi çağıran tutar.
 */
/**
 * İşaretli aralık: boşluksa görünür biçimi ve ekran okuyucu karşılığıyla.
 *
 * Gerçek karakterler DOM'da kalır — paragraf kopyalanınca belgedeki metin
 * birebir gelir. ␣ ve etiketler yalnız CSS `::before` ile çizilir; mark
 * `aria-hidden` olduğu için ekran okuyucu işareti değil "7 boşluk"u duyar.
 * O sözlü metin de kopyaya girmez (`.ws-spoken`, shell.css).
 */
function MarkedSlice({ slice }: { slice: string }) {
  const ws = visibleRange(slice);
  if (!ws) return <mark>{slice}</mark>;
  return (
    <>
      <mark data-visible-ws="" aria-hidden="true">
        {ws.chars.map((c, i) =>
          c.kind === "other" ? (
            c.ch
          ) : (
            <span
              key={i}
              className="ws"
              data-ch={c.kind}
              data-label={c.kind === "invisible" ? c.label : undefined}
            >
              {c.ch}
            </span>
          ),
        )}
      </mark>
      <span className="sr-only ws-spoken">{ws.spoken}</span>
    </>
  );
}

export function FindingList({
  findings,
  blockText,
  activeKey,
  selectedKeys,
  onSetActive,
  onToggleFix,
}: {
  findings: Finding[];
  blockText: Map<string, string>;
  activeKey: string | null;
  selectedKeys: Set<string>;
  onSetActive: (key: string | null) => void;
  onToggleFix: (finding: Finding) => void;
}) {
  return (
    <>
      <h2 className="section-head" id="bulgu-baslik">
        Bulgular
      </h2>
      <ul className="findings" role="list" aria-labelledby="bulgu-baslik">
        {findings.map((f) => {
          const k = findingKey(f);
          const sev = SEVERITY[f.severity];
          const isActive = activeKey === k;
          const text = blockText.get(f.block_id) ?? "";
          const range = markedRange(f.location);
          return (
            <li key={k} className="finding" data-severity={f.severity} data-active={isActive}>
              <button
                type="button"
                className="finding-head"
                aria-expanded={isActive}
                onClick={() => onSetActive(isActive ? null : k)}
              >
                <span className="finding-mark" aria-hidden="true">{sev.mark}</span>
                <span className="finding-sev">{sev.label}</span>
                <span className="finding-title">{f.title}</span>
                <span className="finding-loc">{paragraphNumber(f)}. paragraf</span>
              </button>
              {isActive ? (
                <div className="finding-body">
                  {/* Üç okunur blok: ne yanlış, belgede nerede, ne öneriliyor.
                      Etiket ve boşlukla ayrılır; iç içe kart eklenmez. */}
                  <div className="finding-block">
                    <span className="finding-block-label">Sorun</span>
                    <p className="finding-message">{f.message}</p>
                    <p className="finding-why">{f.explanation}</p>
                  </div>
                  {/* Belgenin kendisi: bulgunun geçtiği paragrafın TAMAMI,
                      işaretli aralık vurgulu. Kırpılmış ±40 karakterlik
                      pencere, bulguyu bağlamından koparıyordu. */}
                  {text ? (
                    <div className="finding-block">
                      <span className="finding-block-label">Belgede</span>
                      <p className="finding-excerpt selectable">
                        {range ? (
                          <>
                            {[...text].slice(0, range[0]).join("")}
                            <MarkedSlice slice={[...text].slice(range[0], range[1]).join("")} />
                            {[...text].slice(range[1]).join("")}
                          </>
                        ) : (
                          text
                        )}
                      </p>
                    </div>
                  ) : null}
                  <div className="finding-block">
                    <span className="finding-block-label">Önerilen düzeltme</span>
                    {f.fix ? (
                      <label className="finding-fix">
                        <input
                          type="checkbox"
                          checked={selectedKeys.has(k)}
                          onChange={() => onToggleFix(f)}
                        />
                        <span>
                          {/* Görünür yazım gözedir; ekran okuyucu ␣ yerine
                              "2 boşluk yerine 1 boşluk" duyar. */}
                          <span aria-hidden="true">
                            <code>{visibleText(f.fix.original)}</code> →{" "}
                            <code>{visibleText(f.fix.replacement)}</code>
                          </span>
                          <span className="sr-only">{spokenFix(f.fix)}</span>
                          <span className="finding-fixnote"> ({f.fix.description})</span>
                        </span>
                      </label>
                    ) : (
                      <p className="finding-nofix">Bu bulgu için otomatik düzeltme önerilmiyor.</p>
                    )}
                  </div>
                </div>
              ) : null}
            </li>
          );
        })}
      </ul>
    </>
  );
}

/**
 * Denetle — belge incelemesi ve cerrahi düzeltme.
 *
 * Zincir: belge açıldı → incele → bulgular → konum → açıklama → önerilen
 * düzeltme → uygula. Düzeltmeler **kopyaya** yazılır; kaynak belge hiçbir
 * zaman yazmak için açılmaz.
 *
 * Bulgular satır içi akordeondur ve öyle kalır: kanıt — belgeden alınan
 * paragraf — bulgunun yanında durmalı, yan panele taşınırsa ikisi ayrılır.
 * Sağ panel yalnız özeti ve düzeltme seçimini taşır; birincil eylem barda.
 * Bulgu yoksa panel de yoktur: gerçek bağlam olmadan üçüncü kolon açılmaz.
 *
 * Parser ve migration ayrıntıları kullanıcıya gösterilmez.
 */
export function ReviewWorkspace({ path }: { path: string }) {
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [busy, setBusy] = useState(true);
  const [failure, setFailure] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [active, setActive] = useState<string | null>(null);
  const [written, setWritten] = useState<string | null>(null);
  // Kurtarılabilir uygulama hatası. `failure`den AYRIDIR: `failure` belge hiç
  // incelenemediğinde çalışma alanının var olamayacağı ölümcül durumdur.
  const [applyError, setApplyError] = useState("");

  const key = findingKey;

  useEffect(() => {
    let cancelled = false;
    setBusy(true);
    setResult(null);
    setFailure(null);
    setSelected(new Set());
    setWritten(null);
    setApplyError("");
    api
      .analyzeDocument(path)
      .then((r) => {
        if (cancelled) return;
        setResult(r);
        setActive(r.findings.length > 0 ? key(r.findings[0]) : null);
        announce(
          r.findings.length === 0
            ? "İnceleme tamamlandı. Bulgu yok."
            : `İnceleme tamamlandı. ${summary(r)}.`,
        );
      })
      .catch((e) => {
        if (cancelled) return;
        // Motor kullanıcı cümlesini kendisi üretiyor; teknik bir şey kalırsa
        // kullanıcıya yedek cümle gider, ham metin log'da durur.
        logFailure("ikincigoz analyze failure", e);
        setFailure(safeMessage(e, "Belge incelenemedi. Farklı bir dosya seçip yeniden deneyin."));
      })
      .finally(() => !cancelled && setBusy(false));
    return () => {
      cancelled = true;
    };
  }, [path]);

  const blockText = useMemo(() => {
    const m = new Map<string, string>();
    result?.blocks.forEach((b) => m.set(b.id, b.text));
    return m;
  }, [result]);

  const fixable = useMemo(
    () => (result?.findings ?? []).filter((f) => f.fix !== null),
    [result],
  );

  const toggle = useCallback((f: Finding) => {
    setSelected((prev) => {
      const next = new Set(prev);
      const k = findingKey(f);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  }, []);

  const chosen: Fix[] = useMemo(
    () => fixable.filter((f) => selected.has(key(f))).map((f) => f.fix as Fix),
    [fixable, selected],
  );

  const applyChosen = useCallback(async () => {
    if (chosen.length === 0) return;
    try {
      // `save()` try'ın DIŞINDAYDI: kaydetme penceresi hiç açılamazsa söz
      // reddi yakalanmıyor, kullanıcı düğmeye basıyor ve hiçbir şey
      // olmuyordu. Vazgeçmek (`!target`) sessizdir; açılamamak değil.
      // Pencere belgenin klasöründe, motorun önerdiği adla ve belgenin
      // türüne süzülmüş açılır (bkz. saveTarget.ts).
      const target = await save(correctedSaveOptions(path, await startFolder(path)));
      if (!target) return;
      // Yeni bir deneme başladı: önceki denemenin makbuzu artık bu ekranın
      // doğrusu değil. Eskiden `written` temizlenmiyordu; ikinci kaydetme
      // düşünce ekranda kırmızı hata ile BİRLİKTE önceki denemenin yeşil
      // "kaydedildi" satırı duruyor, kullanıcı dosyanın yazıldığını
      // sanabiliyordu.
      setApplyError("");
      setWritten(null);
      // Kaydetme penceresi hem KLASÖR hem AD verir. Eskiden yalnız klasör
      // alınıyor, kullanıcının yazdığı ad sessizce atılıyordu; kontrol yalan
      // söylüyordu. Artık ikisi de motora geçer (ad orada güvenli bir dosya
      // adına indirgenir ve no-clobber korunur).
      const cut = Math.max(target.lastIndexOf("/"), target.lastIndexOf("\\"));
      const dir = cut >= 0 ? target.slice(0, cut) : "";
      const chosenName = cut >= 0 ? target.slice(cut + 1) : target;
      const r = await api.applyFixes(path, chosen, dir || undefined, chosenName || undefined);
      setWritten(`${r.fileName} · ${r.applied} düzeltme uygulandı`);
      announce(`${r.applied} düzeltme uygulandı. Kopya kaydedildi.`);
      // Kaynak değişmediği için bulgular geçerliliğini korur; seçim sıfırlanır.
      setSelected(new Set());
    } catch (e) {
      // KURTARILABİLİR hata: kaynak da bulgular da hâlâ geçerli. Eskiden
      // `setFailure` çalışma alanının tamamını (bulgular, seçim, "Kopyaya
      // Uygula") tek bir hata kutusuyla değiştiriyordu; kullanıcı bütün
      // inceleme oturumunu kaybediyordu. Hata artık çalışma alanının içinde
      // gösterilir.
      logFailure("ikincigoz writeback failure", e);
      setWritten(null);
      const message = safeMessage(e, "Düzeltmeler uygulanamadı. Yeniden deneyin.");
      setApplyError(message);
      announce(message);
    }
  }, [chosen, path]);

  if (busy) {
    return (
      <div className="surface">
        <Status tone="busy">Belge inceleniyor…</Status>
      </div>
    );
  }

  if (failure) {
    return (
      <div className="surface">
        <Status tone="error">{failure}</Status>
      </div>
    );
  }

  if (!result) return null;

  return (
    <>
      {/* Barın ortası: denetimin ölçüsü. Bulgu YOKKEN de yazılır — belge
          bağlamı ekranda kalsın diye (§12); yalnız üçüncü kolon açılmaz. */}
      <ToolbarStatus>
        {result.findings.length === 0 ? "Bulgu yok" : summary(result)}
      </ToolbarStatus>

      <ReviewChrome
        result={result}
        fixable={fixable}
        selected={selected}
        onApply={applyChosen}
        onSelectAll={() => setSelected(new Set(fixable.map(key)))}
        onClearSelection={() => setSelected(new Set())}
      />

      <div className="surface review">
        {applyError ? <Status tone="error">{applyError}</Status> : null}
        {written ? <Status tone="success">{written}</Status> : null}

        {result.findings.length === 0 ? (
          <ReviewClear doc={result.document} />
        ) : (
          <FindingList
            findings={result.findings}
            blockText={blockText}
            activeKey={active}
            selectedKeys={selected}
            onSetActive={setActive}
            onToggleFix={toggle}
          />
        )}
      </div>
    </>
  );
}

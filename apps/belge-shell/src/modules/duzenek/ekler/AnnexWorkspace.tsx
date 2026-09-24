/**
 * Dilekçe Ekleri çalışma alanı.
 *
 * DüzenEk'i asıl DüzenEk yapan akış buydu ve birleşmede taşınmamıştı: PDF
 * araçları geldi, ek yönetimi gelmedi. Motor (`crates/ekler-core`) ve 21
 * komut zaten yerindeydi; eksik olan yalnız arayüzdü.
 *
 * Bağımsız uygulamanın EKRANI kopyalanmadı, DAVRANIŞI taşındı. Yerleşim
 * GölgeDosya'nın kendi ölçüsüne uyar: iki sütun, kart yok, gradyan yok.
 *
 * Erişilebilirlik: atama sürükle-bırakla YAPILABİLİR ama sürükle-bırak TEK
 * YOL DEĞİLDİR. Her belgenin yanındaki seçim kutusu aynı işi klavyeyle ve
 * ekran okuyucuyla yapar (§66).
 */
import { useCallback, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { Button, Status } from "../../../shared-ui/primitives";
import { announce } from "../../../shared-ui/Announcer";
import { logFailure, safeMessage } from "../../../shared-ui/failure";
import { ToolbarActions } from "../../../shell/chrome";
import { fileNameOf } from "../../../shell/modes";
import {
  assignSource, assignmentLabel, autoDistribute, createExhibit, exhibitPageCount,
  exhibitsListText, moveExhibit, nameFromFile, removeExhibit, renameExhibit,
  unassignSource, unassignedSources,
} from "./annexState";
import type { AnnexSource, Project, StampConfig } from "./types";
import "./ekler.css";

/**
 * Motorun kendi varsayılanları — uydurma DEĞİL.
 *
 * `ekler_core::DEFAULT_TARGET_SIZE_BYTES` ve `pdf_core::StampConfig::default()`
 * ile birebir aynı. İlk sürümde burada `0` ve `null` vardı: `null` serde
 * tarafından daha komut gövdesine girmeden reddediliyor, `0` ise
 * `validate_project_structure` tarafından. Yani "Ekleri Hazırla ve Kaydet"
 * HER çalıştırmada düşüyordu — üstelik kullanıcıya gösterilen sebep
 * ("Çıktı klasörünü ve belgeleri kontrol edin") yanlış yeri gösteriyordu.
 * Yüzey testi yalnız kaynak metnini aradığı için bunu kaçırdı; sözleşme
 * artık `crates/ekler-core/tests/annex_ipc_contract.rs` ile kilitli.
 */
const DEFAULT_TARGET_SIZE_BYTES = 9_961_472;

const defaultStamp = (): StampConfig => ({
  enabled: true,
  position: "top_right",
  font_size: 10,
  margin_pt: 20,
  show_badge: true,
});

const emptyProject = (): Project => ({
  version: "1.0.0",
  name: "Dilekçe Ekleri",
  created_at: "",
  updated_at: "",
  sources: [],
  exhibits: [],
  target_size_bytes: DEFAULT_TARGET_SIZE_BYTES,
  stamp_config: defaultStamp(),
});

/** Kimlikler saf katmanın dışında üretilir. */
let seq = 0;
const nextId = () => `ek-${(seq += 1)}`;

/**
 * Ek oturumu KİP DEĞİŞİNCE KAYBOLMAMALI.
 *
 * Durum bileşenin içindeyken başka bir kipe geçmek bileşeni söküyor ve
 * kullanıcının kurduğu bütün ek yapısı sessizce siliniyordu: yirmi ekli bir
 * dosya hazırlayan avukat yanlışlıkla "Düzenle"ye tıkladığında hiçbir uyarı
 * almadan her şeyi kaybediyordu. Paketlenmiş uygulamada yapılan smoke bunu
 * ortaya çıkardı. Durum artık kabukta yaşıyor.
 */
export function AnnexWorkspace({ project, onProject }: {
  project: Project;
  onProject: (next: Project | ((current: Project) => Project)) => void;
}) {
  const setProject = onProject;
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [tone, setTone] = useState<"info" | "busy" | "success" | "error">("info");
  const dragged = useRef<string | null>(null);

  const pending = useMemo(() => unassignedSources(project), [project]);
  const totalPages = useMemo(
    () => project.exhibits.reduce((n, e) => n + exhibitPageCount(project, e), 0),
    [project],
  );

  const say = useCallback((text: string, kind: typeof tone) => {
    setStatus(text);
    setTone(kind);
    if (text) announce(text);
  }, []);

  /** Belge ekle: motorun tarayıcısı sayfa sayısını ve imzayı da getirir. */
  const addDocuments = useCallback(async () => {
    try {
      const picked = await open({
        multiple: true,
        directory: false,
        filters: [{ name: "Ek belgesi", extensions: ["pdf", "docx", "doc", "jpg", "jpeg", "png", "tif", "tiff"] }],
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      setBusy(true);
      say("Belgeler inceleniyor…", "busy");
      const result = await invoke<{ sources: AnnexSource[]; errors: { path: string; reason: string }[] }>(
        "duzenek_scan_source_files",
        { paths },
      );
      if (!result.sources.length) throw new Error(result.errors.map((e) => e.reason).join("\n"));
      // Kaynak kimliği İÇERİKTEN türetilir (`src-<sha256>`), dolayısıyla aynı
      // belgeyi iki kez eklemek AYNI kimlikten iki kayıt üretir. Motorun
      // doğrulaması bunu reddeder ("Mükerrer veya boş kaynak kimliği") ve
      // dışa aktarma kalıcı olarak imkânsızlaşırdı. Yinelenen belge sessizce
      // ATLANMAZ, söylenir.
      let duplicates = 0;
      setProject((p) => {
        const known = new Set(p.sources.map((x) => x.id));
        const fresh = result.sources.filter((x) => !known.has(x.id));
        duplicates = result.sources.length - fresh.length;
        return { ...p, sources: [...p.sources, ...fresh] };
      });
      const failed = result.errors.length;
      const added = result.sources.length - duplicates;
      const notes = [
        `${added} belge eklendi.`,
        duplicates ? `${duplicates} belge zaten listedeydi, tekrar eklenmedi.` : "",
        failed
          ? `${failed} dosya okunamadı: ${result.errors.map((e) => fileNameOf(e.path)).join(", ")}`
          : "",
      ].filter(Boolean);
      say(notes.join(" "), failed ? "error" : "success");
    } catch (e) {
      logFailure("ekler scan", e);
      say(safeMessage(e, "Belgeler eklenemedi. Dosyaları kontrol edin."), "error");
    } finally {
      setBusy(false);
    }
  }, [say]);

  const distribute = useCallback(() => {
    if (!pending.length) {
      say("Yüklenen bütün belgeler zaten bir eke atanmış.", "info");
      return;
    }
    setProject((p) => autoDistribute(p, () => nextId()));
    say(`${pending.length} belge yeni eklere dağıtıldı. Elle yaptığınız atamalar korundu.`, "success");
  }, [pending.length, say]);

  const copyList = useCallback(async () => {
    const text = exhibitsListText(project);
    try {
      await navigator.clipboard.writeText(text);
      say("Ekler listesi panoya kopyalandı.", "success");
    } catch (e) {
      logFailure("ekler list copy", e);
      say("Pano kullanılamadı. Liste aşağıda duruyor; elle kopyalayabilirsiniz.", "error");
    }
  }, [project, say]);

  /** Nihai iş: mantıksal plandan gerçek fiziksel çıktılar. */
  const prepare = useCallback(async () => {
    try {
      const dir = await open({ directory: true });
      if (typeof dir !== "string") return;
      setBusy(true);
      say("Ekler hazırlanıyor…", "busy");
      const result = await invoke<{
        outputs: { file_name: string; is_continuation: boolean }[];
        exhibits_list_plain: string;
        package_dir: string;
        /**
         * `ekler_core::ValidationReport`. Maddenin durumu `level`dedir
         * (pass / warning / error) — `passed` diye bir alan YOKTUR. İlk
         * sürüm `passed` okuyordu; `undefined` olduğu için GEÇEN kontroller
         * de "gözden geçirin" diye listeleniyor ve başarılı çalıştırma hata
         * tonuyla gösteriliyordu. Paketlenmiş smoke bunu yakaladı.
         */
        validation_report: {
          is_ready_for_uyap: boolean;
          items: { title: string; level: "pass" | "warning" | "error" }[];
        };
      }>("duzenek_prepare_uyap", { project, outputDir: dir });
      // Motor boyut sınırını aşan eki KENDİSİ böler ve "…_DEVAM_…" dosyaları
      // üretir. Bu, mahkemeye giden dosya kümesini ve EKLER listesini
      // değiştirir; sessizce olmamalı.
      const split = result.outputs.filter((o) => o.is_continuation).length;
      const failures = (result.validation_report?.items ?? []).filter((i) => i.level !== "pass");
      const lines = [
        `${result.outputs.length} dosya üretildi: ${result.package_dir}`,
        split ? `${split} ek boyut sınırı nedeniyle bölündü (DEVAM dosyaları).` : "",
        failures.length ? `Gözden geçirin: ${failures.map((i) => i.title).join(" · ")}` : "",
        "Kaynak belgeleriniz değiştirilmedi.",
      ].filter(Boolean);
      // Ton motorun kendi kararına bağlanır; tek tek maddelerin sayısına değil.
      const ready = result.validation_report?.is_ready_for_uyap !== false;
      say(lines.join("\n"), ready && !failures.length ? "success" : "error");
    } catch (e) {
      logFailure("ekler prepare", e);
      say(safeMessage(e, "Ekler hazırlanamadı. Çıktı klasörünü ve belgeleri kontrol edin."), "error");
    } finally {
      setBusy(false);
    }
  }, [project, say]);

  const canPrepare = project.exhibits.some((e) => e.sources.length > 0);

  return (
    <div className="ekler-root">
      <ToolbarActions>
        <Button disabled={busy || !canPrepare} variant="primary" onClick={() => void prepare()}>
          Ekleri Hazırla ve Kaydet
        </Button>
      </ToolbarActions>

      <Status tone={tone}>{status}</Status>

      <div className="ekler-columns">
        {/* --------------------------------------------- Yüklenen Belgeler */}
        <section className="ekler-pane" aria-labelledby="yuklenen-baslik">
          <header className="ekler-pane-head">
            <h2 id="yuklenen-baslik" className="section-head">Yüklenen Belgeler</h2>
            <span className="ekler-count">
              {project.sources.length === 0
                ? "Belge yok"
                : `${project.sources.length} belge · ${pending.length} atanmadı`}
            </span>
          </header>

          {project.sources.length === 0 ? (
            <p className="ekler-empty">
              Ek olarak kullanacağınız belgeleri ekleyin. Her belge sayfa sayısıyla
              listelenir; sonra istediğiniz eke atarsınız.
            </p>
          ) : (
            <ul className="ekler-docs">
              {project.sources.map((source) => (
                <li
                  key={source.id}
                  className="ekler-doc"
                  draggable
                  onDragStart={() => {
                    dragged.current = source.id;
                  }}
                  onDragEnd={() => {
                    dragged.current = null;
                  }}
                >
                  <span className="ekler-doc-name" title={source.file_name}>{source.file_name}</span>
                  <span className="ekler-doc-meta">
                    {source.page_count} sayfa
                    {source.is_signed ? " · imzalı" : ""}
                  </span>
                  {/* Sürükle-bırak TEK YOL DEĞİL: aynı işi klavyeyle yapar. */}
                  <label className="ekler-assign">
                    <span className="sr-only">{source.file_name} hangi eke atansın</span>
                    <select
                      value={project.exhibits.find((e) => e.sources.some((s) => s.source_id === source.id))?.id ?? ""}
                      onChange={(event) => {
                        const id = event.target.value;
                        setProject((p) => (id ? assignSource(p, source.id, id) : unassignSource(p, source.id)));
                        announce(
                          id
                            ? `${source.file_name}, ${project.exhibits.find((e) => e.id === id)?.order}. eke atandı.`
                            : `${source.file_name} atamasi kaldırıldı.`,
                        );
                      }}
                    >
                      <option value="">Atanmadı</option>
                      {project.exhibits.map((e) => (
                        <option key={e.id} value={e.id}>{`Ek-${e.order}${e.name ? ` — ${e.name}` : ""}`}</option>
                      ))}
                    </select>
                  </label>
                  <span className="ekler-doc-state" data-assigned={assignmentLabel(project, source.id) !== "Atanmadı"}>
                    {assignmentLabel(project, source.id)}
                  </span>
                  {/* Listeden çıkarma yolu OLMALI: yanlış belge eklendiğinde
                      kullanıcının tek çaresi kipten çıkıp baştan başlamaktı. */}
                  <button
                    type="button"
                    className="text-button"
                    aria-label={`${source.file_name} belgesini listeden çıkar`}
                    onClick={() => {
                      setProject((p) => ({
                        ...unassignSource(p, source.id),
                        sources: p.sources.filter((x) => x.id !== source.id),
                      }));
                      announce(`${source.file_name} listeden çıkarıldı.`);
                    }}
                  >
                    Çıkar
                  </button>
                </li>
              ))}
            </ul>
          )}

          {/* İmzalı orijinal, motorda bir ekte TEK BAŞINA durmak zorundadır.
              Kullanıcı politikayı değiştiremezse imzalı belge çıkmazdır:
              başka bir belgeyle aynı eke koyunca sert hata alır ve elinden
              bir şey gelmez. Onay metni Düzenle'deki kardeşiyle aynıdır. */}
          {project.sources.some((s) => s.is_signed) ? (
            <div className="ekler-signed">
              {project.sources.filter((s) => s.is_signed).map((s) => (
                <label key={s.id} className="approval">
                  <input
                    type="checkbox"
                    checked={s.signed_policy === "create_derived_copy"}
                    onChange={(event) => {
                      const derived = event.target.checked;
                      setProject((p) => ({
                        ...p,
                        sources: p.sources.map((x) =>
                          x.id === s.id
                            ? {
                                ...x,
                                signed_policy: derived ? "create_derived_copy" : "use_original_as_is",
                                is_approved_for_conversion: derived,
                              }
                            : x,
                        ),
                      }));
                      announce(
                        derived
                          ? `${s.file_name} için türetilmiş kopya onaylandı.`
                          : `${s.file_name} orijinali olduğu gibi kullanılacak; tek başına bir ek olmalı.`,
                      );
                    }}
                  />
                  {s.file_name}: imza işareti bulundu. Orijinali olduğu gibi
                  kullanılırsa TEK BAŞINA bir ek olmalıdır. Başka belgelerle
                  birlikte kullanmak için türetilmiş kopyaya izin verin —
                  türetilmiş PDF kaynak elektronik imzanın doğrulanabilirliğini
                  taşımaz.
                </label>
              ))}
            </div>
          ) : null}

          <div className="row">
            <Button disabled={busy} onClick={() => void addDocuments()}>Belge Ekle</Button>
            <Button
              variant="quiet"
              disabled={busy || project.sources.length === 0}
              onClick={distribute}
              title="Yalnız atanmamış belgeler için yeni ek açar; elle yaptığınız atamalara dokunmaz"
            >
              Otomatik Dağıt
            </Button>
          </div>
        </section>

        {/* ----------------------------------------------- Dilekçe Ekleri */}
        <section className="ekler-pane" aria-labelledby="ekler-baslik">
          <header className="ekler-pane-head">
            <h2 id="ekler-baslik" className="section-head">Dilekçe Ekleri</h2>
            <span className="ekler-count">
              {project.exhibits.length === 0 ? "Ek yok" : `${project.exhibits.length} ek · ${totalPages} sayfa`}
            </span>
          </header>

          {project.exhibits.length === 0 ? (
            <p className="ekler-empty">
              Dilekçenize koyacağınız ekleri burada oluşturursunuz. Bir ek birden
              fazla belge içerebilir — örneğin “Ek-2 — Banka Dekontları”.
            </p>
          ) : (
            <ol className="ekler-list">
              {project.exhibits.map((exhibit, index) => (
                <li
                  key={exhibit.id}
                  className="ekler-item"
                  onDragOver={(event) => event.preventDefault()}
                  onDrop={(event) => {
                    event.preventDefault();
                    const id = dragged.current;
                    if (!id) return;
                    setProject((p) => assignSource(p, id, exhibit.id));
                    announce(`Belge ${exhibit.order}. eke atandı.`);
                    dragged.current = null;
                  }}
                >
                  <div className="ekler-item-head">
                    <span className="ekler-no">Ek-{exhibit.order}</span>
                    <label className="ekler-name">
                      <span className="sr-only">{`Ek-${exhibit.order} başlığı`}</span>
                      <input
                        value={exhibit.name}
                        placeholder="Başlık"
                        onChange={(event) => setProject((p) => renameExhibit(p, exhibit.id, event.target.value))}
                      />
                    </label>
                    <span className="ekler-pages">{exhibitPageCount(project, exhibit)} sayfa</span>
                  </div>
                  <ul className="ekler-item-sources">
                    {exhibit.sources.length === 0 ? (
                      <li className="ekler-item-empty">Belge atanmadı</li>
                    ) : (
                      exhibit.sources.map((ref) => {
                        const source = project.sources.find((s) => s.id === ref.source_id);
                        return (
                          <li key={ref.source_id}>
                            {source?.file_name ?? ref.source_id}
                            <button
                              type="button"
                              className="text-button"
                              onClick={() => setProject((p) => unassignSource(p, ref.source_id))}
                              aria-label={`${source?.file_name ?? "Belge"} atamasını kaldır`}
                            >
                              Çıkar
                            </button>
                          </li>
                        );
                      })
                    )}
                  </ul>
                  <div className="row">
                    <Button className="btn-sm" variant="quiet" disabled={index === 0}
                      onClick={() => setProject((p) => moveExhibit(p, index, -1))}
                      aria-label={`Ek-${exhibit.order} yukarı taşı`}>↑</Button>
                    <Button className="btn-sm" variant="quiet" disabled={index === project.exhibits.length - 1}
                      onClick={() => setProject((p) => moveExhibit(p, index, 1))}
                      aria-label={`Ek-${exhibit.order} aşağı taşı`}>↓</Button>
                    <Button className="btn-sm" variant="quiet"
                      onClick={() => {
                        setProject((p) => removeExhibit(p, exhibit.id));
                        announce(`Ek-${exhibit.order} kaldırıldı. Belgeleri atanmadı durumuna döndü.`);
                      }}
                      aria-label={`Ek-${exhibit.order} sil`}>Sil</Button>
                  </div>
                </li>
              ))}
            </ol>
          )}

          <div className="row">
            <Button
              disabled={busy}
              onClick={() => {
                const order = project.exhibits.length + 1;
                setProject((p) => createExhibit(p, "", nextId()));
                announce(`Ek-${order} oluşturuldu.`);
              }}
            >
              Ek Ekle
            </Button>
            <Button variant="quiet" disabled={project.exhibits.length === 0} onClick={() => void copyList()}>
              Ekler Listesini Kopyala
            </Button>
          </div>

          {project.exhibits.length > 0 ? (
            <pre className="ekler-preview selectable" aria-label="Ekler listesi önizlemesi">
              {exhibitsListText(project)}
            </pre>
          ) : null}
        </section>
      </div>

      <p className="tool-safe">Yeni dosyalar oluşturulur; kaynak belgeleriniz korunur.</p>
    </div>
  );
}

export { emptyProject, nameFromFile };

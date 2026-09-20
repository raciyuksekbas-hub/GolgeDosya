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

export function AnnexWorkspace() {
  const [project, setProject] = useState<Project>(emptyProject);
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
      setProject((p) => ({ ...p, sources: [...p.sources, ...result.sources] }));
      const failed = result.errors.length;
      say(
        failed
          ? `${result.sources.length} belge eklendi. ${failed} dosya okunamadı: ${result.errors.map((e) => e.path.split("/").pop()).join(", ")}`
          : `${result.sources.length} belge eklendi.`,
        failed ? "error" : "success",
      );
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
      const result = await invoke<{ outputs: { file_name: string }[]; exhibits_list_plain: string; package_dir: string }>(
        "duzenek_prepare_uyap",
        { project, outputDir: dir },
      );
      say(
        `${result.outputs.length} dosya üretildi: ${result.package_dir}\nKaynak belgeleriniz değiştirilmedi.`,
        "success",
      );
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
                </li>
              ))}
            </ul>
          )}

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

export { nameFromFile };

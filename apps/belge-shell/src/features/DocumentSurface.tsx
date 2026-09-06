import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { FeatureState, RecentDocument } from "../shell/types";
import { IconDocumentLarge } from "../shell/icons";
import { announce } from "../shared-ui/Announcer";

/** Her bölümün kabul ettiği belgeler ve yüzey metni. */
const MODES: Record<
  FeatureState["key"],
  { extensions: string[]; label: string; prompt: string; multiple: boolean }
> = {
  duzenek: {
    extensions: ["pdf", "docx", "doc", "udf", "jpg", "jpeg", "png", "tiff", "heic"],
    label: "Belge veya görsel",
    prompt: "Düzenlemek istediğiniz belgeyi açın",
    multiple: true,
  },
  tavzih: {
    extensions: ["docx", "udf"],
    label: "Word veya UYAP belgesi",
    prompt: "Dönüştürmek istediğiniz belgeyi açın",
    multiple: true,
  },
  degisikis: {
    extensions: ["pdf", "docx", "doc", "udf"],
    label: "Karşılaştırılacak belgeler",
    prompt: "Karşılaştırmak için iki belge açın",
    multiple: true,
  },
  ikincigoz: {
    extensions: ["docx", "udf"],
    label: "Word veya UYAP belgesi",
    prompt: "Denetlemek istediğiniz belgeyi açın",
    multiple: false,
  },
};

interface Props {
  feature: FeatureState;
  recents: RecentDocument[];
  onDocuments: (paths: string[]) => void;
  onForget: () => void;
}

function fileName(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] || path;
}

function extension(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/**
 * Belge-merkezli ana yüzey.
 *
 * Uygulamanın açılışta gösterdiği tek şey budur: bir belge açma daveti ve son
 * kullanılanlar. Sürüm, yapılandırma yolu, taşıma durumu, feature bayrağı gibi
 * hiçbir teknik bilgi burada görünmez.
 */
export function DocumentSurface({ feature, recents, onDocuments, onForget }: Props) {
  const mode = MODES[feature.key];
  const [over, setOver] = useState(false);
  const accepted = useRef(mode.extensions);
  accepted.current = mode.extensions;

  const accept = useCallback(
    (paths: string[]) => {
      const usable = paths.filter((p) => accepted.current.includes(extension(p)));
      if (usable.length === 0) {
        announce("Bu bölüm seçilen dosya türünü kabul etmiyor.");
        return;
      }
      onDocuments(mode.multiple ? usable : usable.slice(0, 1));
    },
    [mode.multiple, onDocuments],
  );

  // Native sürükle-bırak. Webview'in kendi olayı kullanılır; HTML5 drop olayı
  // Tauri'de dosya yolunu vermez.
  useEffect(() => {
    let dispose: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") setOver(true);
        else if (event.payload.type === "leave") setOver(false);
        else if (event.payload.type === "drop") {
          setOver(false);
          accept(event.payload.paths);
        }
      })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else dispose = unlisten;
      })
      .catch(() => {
        /* Sürükle-bırak yoksa dosya açma düğmesi tek yoldur; hata gösterilmez. */
      });
    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [accept]);

  const browse = useCallback(async () => {
    const picked = await open({
      multiple: mode.multiple,
      directory: false,
      filters: [{ name: mode.label, extensions: mode.extensions }],
    });
    if (!picked) return;
    accept(Array.isArray(picked) ? picked : [picked]);
  }, [mode, accept]);

  return (
    <div className="doc-surface">
      <div
        className="dropzone"
        data-over={over}
        onDoubleClick={browse}
        role="group"
        aria-label={mode.prompt}
      >
        <IconDocumentLarge className="dropzone-icon" />
        <div className="dropzone-primary">{mode.prompt}</div>
        <div className="dropzone-hint">veya belgeyi buraya bırakın</div>
        <button type="button" className="btn btn-primary" onClick={browse}>
          Dosya Aç…
        </button>
      </div>

      <section className="recents" aria-labelledby="son-baslik">
        <h2 id="son-baslik">Son kullanılanlar</h2>
        {recents.length === 0 ? (
          <p className="recents-empty">Henüz belge açmadınız.</p>
        ) : (
          <>
            <ul className="recents-list">
              {recents.map((r) => (
                <li key={r.path}>
                  <button
                    type="button"
                    className="recent-item"
                    onClick={() => accept([r.path])}
                    title={r.path}
                  >
                    <span className="recent-name">{fileName(r.path)}</span>
                    <span className="recent-kind">{extension(r.path)}</span>
                  </button>
                </li>
              ))}
            </ul>
            <div style={{ marginTop: 8, display: "flex", justifyContent: "flex-end" }}>
              <button type="button" className="btn btn-quiet" onClick={onForget}>
                Listeyi temizle
              </button>
            </div>
          </>
        )}
      </section>
    </div>
  );
}

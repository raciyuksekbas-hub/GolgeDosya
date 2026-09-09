import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { FeatureState, RecentDocument } from "../shell/types";
import { IconDocumentLarge } from "../shell/icons";
import { announce } from "../shared-ui/Announcer";
import { Button, EmptyState, Section, Status } from "../shared-ui/primitives";
import { MODES, extensionOf, fileNameOf, type ContextOutcome } from "../shell/modes";

interface Props {
  feature: FeatureState;
  recents: RecentDocument[];
  /** Kip değişince belgeye ne olduğu. Uyuşmazlık sessizce geçilmez. */
  outcome: ContextOutcome | null;
  /** ⌘O işareti. Her artışta belge seçici açılır; düğmeyle aynı yol. */
  openRequest?: number;
  onDocuments: (paths: string[]) => void;
  onForget: () => void;
}

/** "3 dakika önce", "dün", "12 Eyl" — tam zaman damgası değil. */
function relativeTime(ms: number): string {
  const diff = Date.now() - ms;
  const minute = 60_000;
  if (diff < minute) return "az önce";
  if (diff < 60 * minute) return `${Math.round(diff / minute)} dk önce`;
  if (diff < 24 * 60 * minute) return `${Math.round(diff / (60 * minute))} sa önce`;
  if (diff < 48 * 60 * minute) return "dün";
  return new Date(ms).toLocaleDateString("tr-TR", { day: "numeric", month: "short" });
}

/**
 * Belge yüzeyi — uygulamanın açılış görüntüsü.
 *
 * Dashboard yok. Merkezde tek net görev: belge aç. Son kullanılanlar kart
 * galerisi değil, liste. Tam dosya yolu varsayılan görünümde gösterilmez.
 */
export function DocumentSurface({ feature, recents, outcome, openRequest, onDocuments, onForget }: Props) {
  const mode = MODES[feature.key];
  const [over, setOver] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const accepted = useRef(mode.extensions);
  accepted.current = mode.extensions;

  const accept = useCallback(
    (paths: string[]) => {
      const usable = paths.filter((p) => accepted.current.includes(extensionOf(p)));
      if (usable.length === 0) {
        setRefused("Bu kip seçilen belge türünü açamıyor.");
        announce("Bu kip seçilen belge türünü açamıyor.");
        return;
      }
      setRefused(null);
      onDocuments(mode.needs === 2 ? usable : usable.slice(0, 1));
    },
    [mode.needs, onDocuments],
  );

  // Native sürükle-bırak. HTML5 drop olayı Tauri'de dosya yolunu vermez.
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
        /* Sürükle-bırak yoksa düğme tek yoldur; hata gösterilmez. */
      });
    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [accept]);

  const browse = useCallback(async () => {
    const picked = await open({
      multiple: mode.needs === 2,
      directory: false,
      filters: [{ name: mode.pickerLabel, extensions: mode.extensions }],
    });
    if (!picked) return;
    accept(Array.isArray(picked) ? picked : [picked]);
  }, [mode, accept]);

  // ⌘O düğmenin yaptığı işi yapar; ayrı bir yol açılmadı.
  const firstRender = useRef(true);
  useEffect(() => {
    if (firstRender.current) {
      firstRender.current = false;
      return;
    }
    if (openRequest !== undefined) void browse();
  }, [openRequest, browse]);

  return (
    <div className="surface">
      {/* Kip değişiminde belge taşınamadıysa, sebebi sade biçimde söylenir. */}
      {outcome?.kind === "mismatch" ? (
        <Status tone="info">
          {outcome.rejected.map(fileNameOf).join(", ")} bu kipte açılamıyor.{" "}
          {mode.label} {mode.extensions.slice(0, 4).join(", ").toUpperCase()} belgeleriyle çalışır.
        </Status>
      ) : null}
      {outcome?.kind === "needsMore" ? (
        <Status tone="info">
          Karşılaştırmak için bir belge daha açın.
        </Status>
      ) : null}
      {refused ? <Status tone="error">{refused}</Status> : null}

      <div
        className="dropzone"
        data-over={over}
        onDoubleClick={browse}
        role="group"
        aria-label={mode.prompt}
      >
        <EmptyState
          icon={<IconDocumentLarge />}
          primary={mode.prompt}
          hint="veya buraya sürükleyin"
          actions={
            <Button variant="primary" onClick={browse} title="Belge Aç  ⌘O">
              Belge Aç
            </Button>
          }
        />
      </div>

      {recents.length > 0 ? (
        <Section title="Son kullanılanlar" id="son">
          <ul className="file-list">
            {recents.map((r) => (
              <li key={r.path}>
                <button type="button" className="file-row" onClick={() => accept([r.path])}>
                  <span className="file-name">{fileNameOf(r.path)}</span>
                  <span className="file-kind">{extensionOf(r.path)}</span>
                  <span className="file-time">{relativeTime(r.openedAt)}</span>
                </button>
              </li>
            ))}
          </ul>
          <div style={{ display: "flex", justifyContent: "flex-end", marginTop: "var(--space-2)" }}>
            <Button variant="quiet" onClick={onForget}>
              Listeyi temizle
            </Button>
          </div>
        </Section>
      ) : null}
    </div>
  );
}

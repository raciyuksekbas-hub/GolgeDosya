import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Layout } from "./shell/Layout";
import { PreferencesSheet } from "./shell/Settings";
import { featureForRoute, firstAvailableRoute, resolveRoute } from "./shell/routes";
import type { FeatureState, PrefTab, Settings } from "./shell/types";
import * as api from "./shell/api";
import { applyPreferences, syncWindowTheme } from "./shared-ui/theme";
import { logFailure, safeMessage } from "./shared-ui/failure";
import { DocumentSurface } from "./features/DocumentSurface";
import { ConvertWorkspace } from "./modules/tavzih/ConvertWorkspace";
import { ReviewWorkspace } from "./modules/ikincigoz/ReviewWorkspace";
import { CompareWorkspace } from "./modules/degisikis/CompareWorkspace";
import { PdfWorkspace } from "./modules/duzenek/PdfWorkspace";
import { AnnexWorkspace, emptyProject } from "./modules/duzenek/ekler/AnnexWorkspace";
import type { PreparedAnnex, Project as AnnexProject } from "./modules/duzenek/ekler/types";
import { MODES, carryContext, fileNameOf, visibleRecents, type ContextOutcome } from "./shell/modes";
import { Button, Status } from "./shared-ui/primitives";
import { ConfirmSheet } from "./shared-ui/ConfirmSheet";
import { leaveCopy, leaveLoses, type LeaveAction } from "./shell/leaveGuard";
import { discardsWork } from "./modules/duzenek/ekler/annexState";
import { announce } from "./shared-ui/Announcer";
import { useShortcuts } from "./shared-ui/useShortcuts";

export function App() {
  const [features, setFeatures] = useState<FeatureState[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [route, setRoute] = useState("");
  /** Tercihler penceresi — hangi sekmede açıldığı kenar çubuğundan gelir. */
  const [prefsTab, setPrefsTab] = useState<PrefTab | null>(null);
  /** Sürüm — yalnız Hakkında bölümünde. Yapılandırma dizini GÖSTERİLMEZ. */
  const [version, setVersion] = useState<string | null>(null);
  /**
   * Açık belgeler — kipe değil ORTAMA aittir.
   *
   * Ürünün en önemli davranışı: belge bir kez açılır, kipler arasında gezerken
   * yeniden seçilmez. Eskiden her gezinmede temizleniyordu.
   */
  const [documents, setDocuments] = useState<string[]>([]);
  const [context, setContext] = useState<ContextOutcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** ⌘O sayacı — belge yüzeyine seçiciyi açması için verilen işaret. */
  const [openRequest, setOpenRequest] = useState(0);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        // Eski ayar taşıması açılışta bir kez. Sonucu KULLANICIYA GÖSTERİLMEZ.
        const report = await api.migrateLegacySettings().catch(() => null);
        const [f, s, info] = await Promise.all([
          api.enabledFeatures(),
          api.getSettings(),
          api.appInfo(),
        ]);
        if (cancelled) return;
        setVersion(info.version);
        // Bu üçü yapılandırma dizinini, özellik matrisini ve eski kurulumların
        // yollarını taşır. Konsol da bir yüzeydir: yalnız geliştirmede yazılır.
        if (import.meta.env.DEV) {
          console.debug("[belge] app", info);
          console.debug("[belge] features", f);
          if (report) console.debug("[belge] legacy settings migration", report);
        }
        setFeatures(f);
        setSettings(s);
        setRoute(firstAvailableRoute(f));
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!settings) return;
    return applyPreferences(settings);
  }, [settings]);

  const theme = settings?.theme;
  useEffect(() => {
    if (theme) void syncWindowTheme(theme);
  }, [theme]);

  const active = featureForRoute(route, features);

  /** Ayar yazımı başarısız olursa kullanıcı bunu BİLMELİ. */
  const [settingsError, setSettingsError] = useState("");
  /** Karşılaştır'ın bildirdiği yürürlükteki Temel/Değişik sırası. */
  const [comparePair, setComparePair] = useState<string[] | null>(null);
  /**
   * Dilekçe ekleri oturumu.
   *
   * Kabukta durur ki kip değiştirmek kullanıcının kurduğu ek yapısını
   * silmesin; çalışma alanı sökülüp yeniden kurulduğunda oturum yerinde kalır.
   */
  const [annexProject, setAnnexProject] = useState<AnnexProject>(emptyProject);
  /** Son hazırlanan ek paketi — sonuç yüzeyi de kip değişince kaybolmamalı. */
  const [annexPrepared, setAnnexPrepared] = useState<PreparedAnnex | null>(null);
  /**
   * Kaydedilmemiş çalışma (§54). Çalışma alanları kendi durumlarını bildirir;
   * kaybettiren her eylem (kip değiştirmek, Kapat, ürün işareti, belge
   * değiştirmek, pencereyi kapatmak) önce buraya sorar.
   */
  const [dirty, setDirty] = useState<Partial<Record<string, boolean>>>({});
  const [pending, setPending] = useState<{ title: string; body: string; confirm: string; run: () => void } | null>(null);
  const markDuzenekDirty = useCallback(
    (value: boolean) => setDirty((d) => (d.duzenek === value ? d : { ...d, duzenek: value })),
    [],
  );

  const work = {
    mode: active?.key ?? null,
    dirty,
    annexUnsaved: discardsWork(annexProject, annexPrepared),
  };
  const workRef = useRef(work);
  workRef.current = work;

  /** Eylem kaydedilmemiş çalışmayı silecekse önce sorar; silmeyecekse hemen yapar. */
  const guard = useCallback((action: LeaveAction, run: () => void, target?: string) => {
    if (!leaveLoses(action, workRef.current, target)) {
      run();
      return;
    }
    setPending({
      ...leaveCopy(action),
      run: () => {
        // Onaylanan eylem çalışma alanını söker; bildirim sökülmeden önce silinir.
        setDirty((d) => ({ ...d, [workRef.current.mode ?? ""]: false }));
        run();
      },
    });
  }, []);

  const navigateNow = useCallback(
    (next: string) =>
      setRoute((current) => {
        const resolved = resolveRoute(next, features) || current;
        if (resolved === current) return current;
        const target = features.find((f) => f.route === resolved);
        if (target) {
          // Belge taşınır; yalnız tür uyuşmuyorsa açıkça anlatılır.
          setContext(carryContext(target.key, documents));
        }
        return resolved;
      }),
    [features, documents],
  );

  const navigate = useCallback(
    (next: string) => {
      const resolved = resolveRoute(next, features);
      const target = features.find((f) => f.route === resolved)?.key;
      guard("navigate", () => navigateNow(next), target);
    },
    [features, guard, navigateNow],
  );

  /**
   * Yeni Ekler işlemi: belgeler, ekler, başlıklar, atamalar ve sonuç bırakılır.
   * Tercihler (tema, erişilebilirlik, son belgeler, Dönüştür'ün çıktı klasörü)
   * ayarlarda yaşar ve dokunulmaz; diğer kiplerin açık belgeleri de kalır.
   */
  const newAnnexOperation = useCallback(() => {
    setAnnexProject(emptyProject());
    setAnnexPrepared(null);
    announce("Yeni Ekler işlemi. Belge ekleyerek başlayın.");
  }, []);

  const saveSettings = useCallback(async (next: Settings) => {
    // İyimser güncelleme: arayüz anında tepki verir. Ama yazım başarısız
    // olursa reddetme HİÇBİR YERDE yakalanmıyordu: kullanıcı değişikliği
    // ekranda görüyor, disk'e hiç yazılmamış oluyor ve tercih bir sonraki
    // açılışta sessizce kayboluyordu — sahte başarı. Artık geri alınıyor ve
    // söyleniyor.
    const previous = settings;
    setSettings(next);
    setSettingsError("");
    try {
      setSettings(await api.saveSettings(next));
    } catch (e) {
      logFailure("settings save", e);
      if (previous) setSettings(previous);
      const message = safeMessage(e, "Tercih kaydedilemedi. Değişiklik geri alındı.");
      setSettingsError(message);
      announce(message);
    }
  }, [settings]);

  const openDocuments = useCallback(
    async (paths: string[]) => {
      setDocuments(paths);
      if (active) setContext(carryContext(active.key, paths));
      // Belgeyi AÇMAK ile onu hatırlamak ayrı işlerdir. Hatırlama yazması
      // düşerse (disk dolu, izin, bozuk ayar dosyası) belge yine de açık
      // kalmalı: kullanıcının asıl istediği buydu. Eskiden bu satır
      // korumasızdı; düşünce `openDocuments` sözü reddediliyor, hata hiçbir
      // yere yazılmıyor ve son belgeler listesi sessizce eskimiş kalıyordu.
      try {
        setSettings(await api.rememberDocuments(paths));
      } catch (e) {
        logFailure("recent documents write", e);
        announce("Belge açıldı ancak son belgeler listesine eklenemedi.");
      }
    },
    [active],
  );

  const closeDocuments = useCallback(() => {
    setDocuments([]);
    setContext(null);
    announce("Belge kapatıldı.");
  }, []);

  /** Bardaki "Kapat" ve ürün işareti: kaydedilmemiş iş varsa önce sorar. */
  const requestClose = useCallback((action: LeaveAction = "close") => guard(action, closeDocuments), [guard, closeDocuments]);

  /** Düzenle'nin içinden başka bir belge açmak, açık düzenlemeyi siler. */
  const replaceDocuments = useCallback(
    (paths: string[]) => guard("replace", () => void openDocuments(paths)),
    [guard, openDocuments],
  );

  // Pencereyi kapatmak bütün oturumu siler. Windows'ta yerel başlık
  // çubuğundaki simgeye çift tıklamak bile pencereyi kapatır.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      try {
        const win = getCurrentWindow();
        const off = await win.onCloseRequested((event) => {
          if (!leaveLoses("window", workRef.current)) return;
          event.preventDefault();
          setPending({ ...leaveCopy("window"), run: () => void win.destroy() });
        });
        if (cancelled) off();
        else unlisten = off;
      } catch {
        // Tauri dışında (tarayıcı, test): pencere olayı yok.
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const forgetDocumentsNow = useCallback(async () => {
    setSettingsError("");
    try {
      setSettings(await api.forgetDocuments());
      announce("Son belgeler listesi temizlendi.");
    } catch (e) {
      // Eskiden burada koruma yoktu: kullanıcı "Tümünü Temizle" diyor, liste
      // olduğu gibi duruyor ve hiçbir şey söylenmiyordu. Silinmediyse
      // söylenir — "sildim" demeyen bir liste, sildiğini sanan bir
      // kullanıcıdan iyidir.
      logFailure("forget documents", e);
      const message = safeMessage(e, "Liste temizlenemedi. Belgeler hatırlanmaya devam ediyor.");
      setSettingsError(message);
      announce(message);
    }
  }, []);

  /** Listeyi temizlemek geri alınamaz: her seferinde sorulur (§54). */
  const forgetDocuments = useCallback(() => {
    setPending({
      title: "Son belgeler listesi temizlensin mi?",
      body: "Bütün kiplerdeki son belgeler listesi temizlenir. Belgeleriniz silinmez. Bu işlem geri alınamaz.",
      confirm: "Listeyi Temizle",
      run: () => void forgetDocumentsNow(),
    });
  }, [forgetDocumentsNow]);

  /** Bu kip, açık belgelerle şu an çalışabiliyor mu? */
  const usable = useMemo(() => {
    if (!active || documents.length === 0) return false;
    const outcome = context ?? carryContext(active.key, documents);
    return outcome.kind === "keep";
  }, [active, documents, context]);

  // macOS'un öğrettiği iki kısayol. Sheet açıkken kapalı: odak tuzağının
  // içinden arka plandaki eylemi tetiklemek odak modelini bozar.
  //
  // ⌘O'yu KİMSE dinlemiyorken bağlamayız. `openRequest` yalnız belge
  // yüzeyinde tüketilir; bir çalışma alanı açıkken o yüzey DOM'da değildir.
  // Kısayol yine de eşleşiyor, `preventDefault()` çalışıyor ve sayaç
  // kimsenin okumadığı bir yere artıyordu: tuş yutuluyor, karşılığında
  // hiçbir şey olmuyordu. Belge açıkken başka belgeye geçmenin yolu bardaki
  // "Kapat" düğmesidir ve klavyeyle erişilebilir.
  useShortcuts(
    useMemo(
      () => [
        { key: "o", run: () => setOpenRequest((n) => n + 1), enabled: !prefsTab && !usable },
        { key: ",", run: () => setPrefsTab("genel") },
      ],
      [prefsTab, usable],
    ),
  );

  /**
   * Yardımcı barın sol ucu: açık belgenin adı; belge yokken kipin adı.
   */
  const barContext = useMemo(() => {
    // Belge yokken bar boş bir bant değildir: kipin adını taşır.
    if (documents.length === 0) {
      return active ? <span className="toolbar-mode">{MODES[active.key].label}</span> : null;
    }
    // Yalnız bu kipin GERÇEKTEN kullandığı belgeler. Seçici çoklu seçime izin
    // veriyor; yedi dosya seçilince bar yedi kırpılmış çipe dönüşüyordu.
    // Fazlalıklar atılmaz — kullanıcı geri döndüğünde yine oradalar.
    let shown = documents.slice(0, active ? MODES[active.key].needs : 1);
    // Karşılaştır'da sürüm değiştirilmiş olabilir. Çalışma alanı kendi
    // sırasını bildirir; bar onu izler, yoksa çipler açılış sırasında kalır ve
    // "önce yazan Temel'dir" okumasıyla çelişir. Bildirilen sıra ancak AYNI
    // belgelerin bir dizilişiyse kullanılır: yeni bir çift açıldığında eski
    // bildirim kendiliğinden düşer.
    if (
      comparePair &&
      comparePair.length === shown.length &&
      comparePair.every((p) => shown.includes(p))
    )
      shown = comparePair;
    return shown.map((p, i) => (
      <span key={p} className="doc-chip" title={fileNameOf(p)}>
        {i > 0 ? (
          <span className="doc-chip-sep" aria-hidden="true">
            ↔
          </span>
        ) : null}
        <span className="doc-chip-name">{fileNameOf(p)}</span>
      </span>
    ));
  }, [documents, active, comparePair]);

  if (error) {
    return (
      <Layout
        features={features}
        current={route}
        onNavigate={navigate}
        onOpenSettings={setPrefsTab}
        // Ürün işareti: kipin başlangıç yüzeyine (son belgeler) döner. Yalnız
        // dönülecek bir yer varken düğmedir; Ekler'de anlamı yoktur (madde 29).
        onHome={documents.length > 0 && active && active.key !== "ekler" ? () => requestClose("home") : undefined}
      >
        <div className="surface">
          <Status tone="error">Uygulama başlatılamadı. Lütfen yeniden açmayı deneyin.</Status>
        </div>
      </Layout>
    );
  }

  const outcome = active && documents.length > 0 ? context ?? carryContext(active.key, documents) : null;
  // Kabuğun TAŞIMA KARARI motora da geçer. `carryContext` "bu kip bir belge
  // ister, ilkini taşı" diyip bar da onu gösterirken, çalışma alanlarına ham
  // `documents` dizisi veriliyordu: Karşılaştır'dan iki belgeyle Dönüştür'e
  // geçen kullanıcıya bar bir belge gösteriyor, motor ikisini birden
  // dönüştürüp haber verilmemiş bir ZIP üretiyordu. Karar tek yerde verilir,
  // her yerde aynı uygulanır. (İkinci belge ATILMAZ; `documents` korunur,
  // kullanıcı geri döndüğünde yine oradadır.)
  const carried = outcome && outcome.kind === "keep" ? outcome.paths : documents;

  return (
    <>
      <Layout
        features={features}
        current={route}
        onNavigate={navigate}
        onOpenSettings={setPrefsTab}
        context={barContext}
        actions={
          // Home'da bar boştur: tek birincil eylem boş durumdadır (§6, §22).
          // Ekler kabuğun belgelerini kullanmaz: oradaki "Kapat" başka bir
          // kipin belgesini kapatıyor, Ekler'e hiçbir şey yapmıyordu ve
          // kullanıcıya sahte bir çıkış gibi görünüyordu (madde 19).
          documents.length > 0 && active?.key !== "ekler" ? (
            <Button variant="quiet" onClick={() => requestClose()}>
              Kapat
            </Button>
          ) : null
        }
      >
        {active && settings ? (
          // Ekler kendi belge seçicisiyle çalışır: kabuğun tek-belge akışına
          // girmez, bu yüzden `usable` kapısının ÖNÜNDE durur.
          active.key === "ekler" ? (
            <AnnexWorkspace
              project={annexProject}
              onProject={setAnnexProject}
              prepared={annexPrepared}
              onPrepared={setAnnexPrepared}
              onNewOperation={newAnnexOperation}
            />
          ) : usable && active.key === "tavzih" ? (
            <ConvertWorkspace paths={carried} onNewConversion={closeDocuments} />
          ) : usable && active.key === "ikincigoz" ? (
            <ReviewWorkspace path={carried[0]} />
          ) : usable && active.key === "degisikis" ? (
            <CompareWorkspace paths={carried} onPairChange={setComparePair} onReplaceDocuments={openDocuments} />
          ) : usable && active.key === "duzenek" ? (
            // Açılamayan belgeden kurtulma yolu kabuktan geçer: bardaki ad, son
            // kullanılanlar ve çalışma alanı aynı belgeyi göstersin.
            <PdfWorkspace paths={carried} onOpenDocument={replaceDocuments} onDirtyChange={markDuzenekDirty} />
          ) : (
            <DocumentSurface
              feature={active}
              recents={visibleRecents(settings)}
              outcome={outcome}
              openRequest={openRequest}
              onDocuments={openDocuments}
              onForget={forgetDocuments}
            />
          )
        ) : null}
      </Layout>
      {prefsTab && settings ? (
        <PreferencesSheet
          settings={settings}
          version={version}
          tab={prefsTab}
          onTab={setPrefsTab}
          onChange={saveSettings}
          error={settingsError}
          onForget={forgetDocuments}
          onClose={() => setPrefsTab(null)}
        />
      ) : null}
      {pending ? (
        <ConfirmSheet
          title={pending.title}
          confirmLabel={pending.confirm}
          onCancel={() => setPending(null)}
          onConfirm={() => {
            const run = pending.run;
            setPending(null);
            run();
          }}
        >
          {pending.body}
        </ConfirmSheet>
      ) : null}
    </>
  );
}

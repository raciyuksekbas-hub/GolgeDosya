import { useCallback, useEffect, useMemo, useState } from "react";
import { Layout } from "./shell/Layout";
import { PreferencesSheet } from "./shell/Settings";
import { featureForRoute, firstAvailableRoute, resolveRoute } from "./shell/routes";
import type { FeatureState, PrefTab, Settings } from "./shell/types";
import * as api from "./shell/api";
import { applyPreferences } from "./shared-ui/theme";
import { logFailure, safeMessage } from "./shared-ui/failure";
import { DocumentSurface } from "./features/DocumentSurface";
import { ConvertWorkspace } from "./modules/tavzih/ConvertWorkspace";
import { ReviewWorkspace } from "./modules/ikincigoz/ReviewWorkspace";
import { CompareWorkspace } from "./modules/degisikis/CompareWorkspace";
import { PdfWorkspace } from "./modules/duzenek/PdfWorkspace";
import { AnnexWorkspace } from "./modules/duzenek/ekler/AnnexWorkspace";
import { MODES, carryContext, fileNameOf, type ContextOutcome } from "./shell/modes";
import { Button, Status } from "./shared-ui/primitives";
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

  const active = featureForRoute(route, features);

  const navigate = useCallback(
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

  /** Ayar yazımı başarısız olursa kullanıcı bunu BİLMELİ. */
  const [settingsError, setSettingsError] = useState("");
  /** Karşılaştır'ın bildirdiği yürürlükteki Temel/Değişik sırası. */
  const [comparePair, setComparePair] = useState<string[] | null>(null);

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

  const forgetDocuments = useCallback(async () => {
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
          documents.length > 0 ? (
            <Button variant="quiet" onClick={closeDocuments}>
              Kapat
            </Button>
          ) : null
        }
      >
        {active && settings ? (
          // Ekler kendi belge seçicisiyle çalışır: kabuğun tek-belge akışına
          // girmez, bu yüzden `usable` kapısının ÖNÜNDE durur.
          active.key === "ekler" ? (
            <AnnexWorkspace />
          ) : usable && active.key === "tavzih" ? (
            <ConvertWorkspace paths={carried} />
          ) : usable && active.key === "ikincigoz" ? (
            <ReviewWorkspace path={carried[0]} />
          ) : usable && active.key === "degisikis" ? (
            <CompareWorkspace paths={carried} onPairChange={setComparePair} />
          ) : usable && active.key === "duzenek" ? (
            // Açılamayan belgeden kurtulma yolu kabuktan geçer: bardaki ad, son
            // kullanılanlar ve çalışma alanı aynı belgeyi göstersin.
            <PdfWorkspace paths={carried} onOpenDocument={openDocuments} />
          ) : (
            <DocumentSurface
              feature={active}
              recents={settings.recentDocuments}
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
    </>
  );
}

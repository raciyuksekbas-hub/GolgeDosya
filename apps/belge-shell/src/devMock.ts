/**
 * Geliştirme-yalnız IPC taklidi.
 *
 * Tauri komutları yalnız paketlenmiş uygulamada vardır. Tarayıcıda kabuk
 * açılışta düşer ve arayüz hiç çizilmez; bu da klavye akışı, odak tuzağı ve
 * hareket/kontrast tercihlerinin GERÇEK etkileşimle denenmesini imkânsız
 * kılıyordu.
 *
 * Bu modül yalnız `import.meta.env.DEV` iken ve yalnız Tauri YOKKEN devreye
 * girer. Üretim paketinde `main.tsx` onu hiç çağırmaz, dolayısıyla pakete
 * girmez. Motorları taklit etmez — yalnız kabuğun açılabilmesi için gereken
 * en küçük yüzeyi verir; belge açma gibi işler burada çalışmaz.
 */

const RECENTS = [
  { path: "/Users/örnek/Belgeler/İŞ SÖZLEŞMESİ.docx", openedAt: Date.now() - 9e5 },
  { path: "/Users/örnek/Belgeler/dilekçe-taslak.udf", openedAt: Date.now() - 7e6 },
];

const SETTINGS = {
  theme: "system",
  textScale: 100,
  highContrast: "system",
  reduceMotion: "system",
  respectReducedMotion: true,
  acceptedTerms: 1,
  outputDir: null,
  rendererPath: null,
  disabledRules: [],
  includeReview: true,
  sourceReadOnly: true,
  linearResults: false,
  recentDocuments: RECENTS,
  migratedFrom: [],
};

const RESPONSES: Record<string, unknown> = {
  app_info: { name: "Yüksekbaş Belge", version: "0.0.1", nameIsProvisional: true, configDir: "" },
  enabled_features: [
    { key: "duzenek", label: "Düzenle", route: "duzenek", compiled: true, enabled: true },
    { key: "tavzih", label: "Dönüştür", route: "tavzih", compiled: true, enabled: true },
    { key: "degisikis", label: "Karşılaştır", route: "degisikis", compiled: true, enabled: true },
    { key: "ikincigoz", label: "Denetle", route: "ikincigoz", compiled: true, enabled: true },
  ],
  get_settings: SETTINGS,
  save_settings: SETTINGS,
  remember_documents: SETTINGS,
  forget_documents: { ...SETTINGS, recentDocuments: [] },
  migrate_legacy_settings: { changed: false, sources: [] },
};

export function installDevMock() {
  const w = window as unknown as Record<string, unknown>;
  if (w.__TAURI_INTERNALS__) return; // Gerçek Tauri varsa asla araya girme.
  w.__TAURI_INTERNALS__ = {
    invoke: (cmd: string) =>
      cmd in RESPONSES
        ? Promise.resolve(RESPONSES[cmd])
        : // Kayıtsız her komut REDDEDİLİR. Sessizce başarı döndürmek, gerçek
          // uygulamada olmayan bir davranışı varmış gibi gösterirdi.
          Promise.reject(new Error(`dev taklidinde yok: ${cmd}`)),
    transformCallback: (cb: unknown) => cb,
    // `getCurrentWebview()` bu üstveriyi okur; olmadan belge yüzeyi çizilmeden
    // düşer. Sürükle-bırak aboneliği reddedilir ve çağıran zaten yakalar.
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
  };
  console.debug("[belge] geliştirme IPC taklidi etkin — üretimde bulunmaz");
}

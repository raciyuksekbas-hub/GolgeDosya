// "Son belgeleri göster" (saha maddesi 11).
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { visibleRecents } from "./modes";
import { PreferencesSheet } from "./Settings";
import type { Settings } from "./types";

const recents = [{ path: "C:\\Users\\Çağrı Şahin\\Belgeler\\dilekçe.pdf", openedAt: 1 }];

describe("son belgeler görünürlüğü", () => {
  it("kapalıyken liste boş döner; açıkken ya da alan yokken aynen", () => {
    expect(visibleRecents({ showRecents: false, recentDocuments: recents })).toEqual([]);
    expect(visibleRecents({ showRecents: true, recentDocuments: recents })).toBe(recents);
    expect(visibleRecents({ recentDocuments: recents })).toBe(recents);
  });
});

describe("Tercihler > Genel", () => {
  const settings = {
    theme: "system", textScale: 100, highContrast: "system", reduceMotion: "system",
    respectReducedMotion: true, recentDocuments: recents, showRecents: false,
  } as unknown as Settings;
  const html = renderToStaticMarkup(
    <PreferencesSheet settings={settings} tab="genel" onTab={() => {}} onChange={() => {}} error="" onForget={() => {}} onClose={() => {}} />,
  );

  it("gerçek onay kutusu, etiketiyle; kapalıyken işaretsiz", () => {
    expect(html).toMatch(/<label class="field prefs-check">[\s\S]*Son belgeleri göster[\s\S]*<input type="checkbox"\/><\/label>/);
  });

  it("gizlemek silmek değildir: sayı ve 'Listeyi Temizle' durur, açıklama bunu söyler", () => {
    expect(html).toContain("1 belge hatırlanıyor");
    expect(html).toContain("Listeyi Temizle");
    expect(html).toContain("mevcut liste silinmez");
  });
});

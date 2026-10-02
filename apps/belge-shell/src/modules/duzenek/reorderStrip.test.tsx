// a06 — Sırala: şeridin işaretlemesi ve etkileşim sözleşmesi.
//
// Saf sıra modeli pageOrder.test.ts'te. Burada küçük resmin işaretlemesi
// (liste rolleri, iç içe denetim yok, seçim çerçevesi yalnız seçimi olan
// araçlarda), sürüklemenin işaretçi olaylarıyla yapıldığı, klavyeyle taşıma,
// duyuru ve Geri Al kilitlenir. Gerçek işaretçi sürüklemesi headless Chrome
// düzeneğinde denenir (v032 kanıt dizini).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PageThumbnail, PreviewStage } from "./PdfWorkspace";
import { DRAG_THRESHOLD, dragGesture, REFUSE_DISTANCE } from "./useStripDrag";
import * as workspaceState from "./pdfWorkspaceState";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(here, "PdfWorkspace.tsx"), "utf8");
const drag = readFileSync(resolve(here, "useStripDrag.ts"), "utf8");

describe("şeridin işaretlemesi", () => {
  const src = { id: "a", path: "/belgeler/dilekce.pdf", file_name: "dilekce.pdf", format: "pdf", size_bytes: 1, page_count: 3, is_signed: false } as never;
  const item = { source: src, page: 2, key: "a#2" };
  const render = (kind: string, extra: Record<string, unknown> = {}) => renderToStaticMarkup(
    <PageThumbnail item={item} index={1} count={3} kind={kind as never} isCurrent={false} isSelected rotation={0} busy={false}
      onView={() => {}} onToggle={() => {}} onShift={() => {}} {...extra} />,
  );

  it("sayfa bir liste öğesidir; şerit role=list taşır", () => {
    expect(render("reorder")).toMatch(/^<div role="listitem" class="thumbnail /);
    expect(source).toMatch(/<aside className="thumbnail-list" aria-label="Sayfa önizlemeleri"[^>]*>\s*<div role="list">/);
  });
  it("Sırala: klavyeyle seçim için Seç kutusu, ↑/↓ ve sürükleme tutamağı yan yana; iç içe etkileşim yok", () => {
    const html = render("reorder");
    expect(html).toContain('aria-label="2. sayfa (kaynak s. 2) — Seç"');
    expect(html).toContain('aria-label="2. sayfayı yukarı taşı (kaynak s. 2)"');
    expect(html).toContain('aria-label="2. sayfayı aşağı taşı (kaynak s. 2)"');
    // Görüntüle düğmesinin içinde başka bir denetim yok.
    const button = html.slice(html.indexOf('<button class="thumbnail-image-button"'), html.indexOf("</button>") + 9);
    expect(button).not.toMatch(/<(button|input|a)\b[^>]*>[\s\S]*<(button|input|a)\b/);
  });
  it("seçim çerçevesi yalnız seçimi olan araçlarda; Birleştir ve Döndür'de ilk sayfa işaretli görünmez", () => {
    for (const kind of ["merge", "rotate"]) expect(render(kind)).not.toMatch(/class="thumbnail [^"]*selected/);
    for (const kind of ["select", "delete", "reorder"]) expect(render(kind)).toMatch(/class="thumbnail [^"]*selected/);
    // Seç/Sil'in kutu adları değişmedi (WCAG 2.5.3).
    expect(render("select")).toContain("— Dahil et");
    expect(render("delete")).toContain("— Çıkar");
  });
  it("bırakma çizgisi ve sürüklenen sayfa işaretlenir", () => {
    expect(render("reorder", { drop: "before", dragging: true })).toMatch(/data-drop="before" data-dragging="true"/);
    expect(render("reorder")).not.toContain("data-drop");
  });
  it("önizleme görseli yerel olarak sürüklenmez (her kipte)", () => {
    const html = renderToStaticMarkup(
      <PreviewStage large={false} geometry={{ width: 80, height: 100, imageWidth: 80, imageHeight: 100, scale: 1 }} bitmap={{ url: "blob:x" }} error="" rotation={0} label="s" />,
    );
    expect(html).toMatch(/<img [^>]*draggable="false"/);
  });
});

describe("sürükleme ve klavye sözleşmesi", () => {
  it("işaretçi olaylarıyla sürüklenir; HTML5 sürükle-bırak kullanılmaz", () => {
    for (const marker of ["setPointerCapture", "'pointermove'", "'pointerup'", "'pointercancel'", "'Escape'", "DRAG_THRESHOLD = 4", "pointerType === 'touch'"])
      expect(drag).toContain(marker);
    expect(source + drag).not.toMatch(/onDragStart|onDrop=|draggable=\{true\}|dataTransfer/);
    // Sürüklemeden sonra gelen tıklama yutulur.
    expect(source).toContain("onClickCapture={strip.onClickCapture}");
    expect(drag).toMatch(/suppressClick\.current = true/);
  });
  it("Finder/Explorer'dan dosya bırakarak açma korunur", () => {
    const conf = JSON.parse(readFileSync(resolve(here, "../../../src-tauri/tauri.conf.json"), "utf8"));
    expect(JSON.stringify(conf)).toContain('"dragDropEnabled":true');
  });
  it("seçili sayfalar fare olmadan taşınır; her taşıma duyurulur ve geri alınabilir", () => {
    expect(source).toMatch(/role="group" aria-label="Seçili sayfaları taşı"/);
    for (const label of [">Başa<", ">Yukarı<", ">Aşağı<", ">Sona<", ">Geri Al<"]) expect(source).toContain(label);
    expect(source).toContain("title={`Son taşımayı geri al  ${shortcutLabel('Z')}`}");
    expect(source).toMatch(/const applyMove = [\s\S]*?pushHistory[\s\S]*?announce\(describeMove/);
    // ⌘Z yalnız Sırala'da etkin (kısayol politikası: sistemin öğrettiği anlam, ipucunda yazılı).
    expect(source).toMatch(/useShortcuts\([\s\S]*?key: 'z'[\s\S]*?kind === 'reorder'\)/);
  });
  it("görüntüle düğmesi klavyeyle (detail 0) seçimi değiştirmez; fare tıklaması masaüstü listesi gibi seçer", () => {
    const view = source.slice(source.indexOf("const viewPage = "), source.indexOf("const stripRef = "));
    expect(view).toContain("keyboard: event.detail === 0");
    expect(view).toContain("toggle: isToggleChord(event)");
  });
  it("platform ayrımı arayüze sızmaz", () => {
    expect(source).toContain("isToggleChord(event)");
    expect(source + drag).not.toMatch(/navigator\.platform|userAgent|isWindows|isMac/i);
  });
});

describe("Sırala'da şerit tek sekme durağıdır (roving tabindex)", () => {
  // 60 sayfalık bir ekte 3. sayfayı seçip "Sona"ya ulaşmak sayfa başına dört
  // durakla ~228 Tab istiyordu. Artık yalnız bir sayfanın denetimleri sekme
  // sırasında; ↑/↓, Home/End sayfalar arasında gezdirir.
  const src = { id: "a", path: "/b/d.pdf", file_name: "d.pdf", format: "pdf", size_bytes: 1, page_count: 3, is_signed: false } as never;
  const thumb = (kind: string, extra: Record<string, unknown> = {}) => renderToStaticMarkup(
    <PageThumbnail item={{ source: src, page: 2, key: "a#2" }} index={1} count={3} kind={kind as never} isCurrent={false} isSelected={false} rotation={0} busy={false}
      onView={() => {}} onToggle={() => {}} onShift={() => {}} {...extra} />,
  );
  const controls = (html: string) => [...html.matchAll(/<(button|input)\b[^>]*>/g)].map((m) => m[0]);

  it("sekme sırası dışındaki sayfanın dört denetimi de tabindex=-1 taşır; durak sayfası ve öteki araçlar değişmez", () => {
    const off = controls(thumb("reorder", { tabbable: false }));
    expect(off).toHaveLength(4);
    for (const c of off) expect(c).toContain('tabindex="-1"');
    for (const c of controls(thumb("reorder", { tabbable: true }))) expect(c).not.toContain("tabindex");
    for (const c of controls(thumb("select"))) expect(c).not.toContain("tabindex");
  });
  it("bileşen yalnız Sırala'da tek durak bırakır ve oklarla gezdirir", () => {
    expect(source).toContain("tabbable={kind !== 'reorder' || item.key === rovingKey}");
    expect(source).toMatch(/<aside className="thumbnail-list"[^>]*onKeyDown=\{stripKeys\}[^>]*onFocus=\{stripFocus\}/);
    const keys = source.slice(source.indexOf("const stripKeys = "), source.indexOf("const strip = useStripDrag"));
    expect(keys).toContain("kind !== 'reorder'");
    expect(keys).toContain("stripStep(event.key");
    expect(keys).toContain("event.preventDefault()");
    // Ekran okuyucuya yol söylenir.
    expect(source).toContain("Sayfalar arasında ↑ ve ↓ tuşlarıyla geçilir");
  });
});

describe("tıklama mı sürükleme mi", () => {
  it("Sırala'da ~4 px'ten sonrası sürüklemedir", () => {
    expect(DRAG_THRESHOLD).toBe(4);
    expect(dragGesture(3.9, true)).toBe("pending");
    expect(dragGesture(4, true)).toBe("drag");
  });
  it("başka araçta hareket sürükleme değildir; yalnız belirgin bir sürükleme ipucu doğurur", () => {
    expect(dragGesture(4, false)).toBe("pending");
    expect(dragGesture(REFUSE_DISTANCE - 1, false)).toBe("pending");
    expect(dragGesture(REFUSE_DISTANCE, false)).toBe("refuse");
  });
  it("sürükleme ipucu yalnız Sırala'nın açık sayfaları aynen tutacağı yerde: tek belge, Görseller değil", () => {
    // Birleştir'de A+B+C varken ipucuna uyup Sırala'ya geçen kullanıcı B ve C'yi
    // kaybediyordu; Görseller'de seçilen görseller düşüyordu.
    const f = (workspaceState as Record<string, unknown>).dragHint;
    expect(typeof f, "dragHint pdfWorkspaceState'te tanımlı olmalı").toBe("function");
    const hint = f as (kind: string, sourceCount: number) => string;
    const text = "Sayfaları sürükleyerek sıralamak için Sırala aracını seçin.";
    for (const kind of ["merge", "select", "delete", "rotate", "compress", "crop", "watermark", "number", "raster"]) expect(hint(kind, 1), kind).toBe(text);
    expect(hint("merge", 2)).toBe("");
    expect(hint("merge", 3)).toBe("");
    expect(hint("images", 1)).toBe("");
    expect(hint("images", 4)).toBe("");
    expect(hint("reorder", 1)).toBe("");
    const refused = source.slice(source.indexOf("onRefused: () => {"), source.indexOf("const relist = "));
    expect(refused).toContain("dragHint(kind, sources.length)");
    expect(refused).not.toContain("'Sayfaları sürükleyerek");
  });
  it("tıklama yalnız gerçek bir sürüklemeden sonra yutulur", () => {
    // Birleştir'de titrek bir tıklama önizlenen sayfayı yine değiştirmeli.
    const move = drag.slice(drag.indexOf("const move = "), drag.indexOf("const up = "));
    expect(move.indexOf("suppressClick.current = true")).toBeGreaterThan(move.indexOf("gesture === 'refuse'"));
    expect(move.match(/suppressClick\.current = true/g)).toHaveLength(1);
  });
});

describe("klavye odağı taşımadan sonra yerinde kalır", () => {
  it("taşıma düğmeleri kullanılamazken sönmez, odakta kalır (aria-disabled)", () => {
    const controls = source.slice(source.indexOf('<div className="row reorder-controls">'), source.indexOf("Geri Al</Button>"));
    for (const how of ["start", "up", "down", "end"]) expect(controls).toContain(`aria-disabled={!canMove('${how}') || undefined}`);
    expect(controls).toContain("aria-disabled={busy || !history.length || undefined}");
    expect(controls).not.toMatch(/\sdisabled=\{/);
    // aria-disabled tıklamayı durdurmaz: işlem kendi koşulunu denetler.
    expect(source).toMatch(/const moveSelection = [^\n]*if \(canMove\(how\)\)/);
    expect(source).toMatch(/const undo = \(\) => \{\s*if \(busy \|\| !history\.length\) return;/);
  });
  it("sayfanın ↑/↓ düğmesiyle taşınan sayfanın aynı oku odakta kalır", () => {
    expect(source).toContain("onShift={delta => shiftFromStrip(item, index, delta)}");
    expect(source).toMatch(/if \(stripRef\.current\?\.contains\(document\.activeElement\)\) refocus\.current = \{ key: item\.key, delta \};/);
    expect(source).toContain("querySelectorAll<HTMLButtonElement>('.thumbnail-step button')[want.delta < 0 ? 0 : 1]");
  });
  it("taşınan ilk sayfa şeritte görünür kalır", () => {
    expect(source).toMatch(/const applyMove = [\s\S]*?revealPage\.current = moved\[0\]\.key;/);
    expect(source).toContain("if (shown) item(shown)?.scrollIntoView?.({ block: 'nearest' });");
  });
  it("kenardaki ok sönük görünür ama odakta kalır; basılınca sayfa yerinde durur", () => {
    const at = (index: number) => renderToStaticMarkup(
      <PageThumbnail item={{ source: { id: "a", path: "/b/d.pdf", file_name: "d.pdf", format: "pdf", size_bytes: 1, page_count: 3, is_signed: false } as never, page: 1, key: "a#1" }}
        index={index} count={3} kind="reorder" isCurrent={false} isSelected={false} rotation={0} busy={false} onView={() => {}} onToggle={() => {}} onShift={() => {}} />,
    );
    const arrows = (html: string) => [...html.matchAll(/<button[^>]*class="icon-btn"[^>]*>/g)].map((m) => m[0]);
    const [up, down] = arrows(at(0));
    expect(up).toContain('aria-disabled="true"');
    expect(down).not.toContain("aria-disabled");
    expect(arrows(at(2))[1]).toContain('aria-disabled="true"');
    for (const button of [...arrows(at(0)), ...arrows(at(2))]) expect(button).not.toMatch(/\sdisabled=""/);
    expect(source).toContain("onClick={() => index > 0 && !busy && onShift(-1)}");
    expect(source).toContain("onClick={() => index < count - 1 && !busy && onShift(1)}");
  });
});

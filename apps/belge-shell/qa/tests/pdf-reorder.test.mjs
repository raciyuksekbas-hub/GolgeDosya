// Sırala — sürükleyerek sıralamanın görsel sözleşmesi (a06).
//
// Bırakma yeri iki sayfa arasındaki 2 px'lik vurgu çizgisiyle gösterilir;
// gölge kart, kayan sayfa ya da animasyon yok. Sürüklenen sayfa soluklaşır.
// Hareket olmadığı için azaltılmış hareket tercihinde görünüm aynıdır.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import postcss from "postcss";

const css = readFileSync("src/modules/duzenek/pdf.css", "utf8");
const rules = new Map();
postcss.parse(css).walkRules((rule) => {
  const body = {};
  rule.walkDecls((d) => { body[d.prop] = d.value; });
  for (const selector of rule.selectors) rules.set(selector, { ...(rules.get(selector) || {}), ...body });
});
const rule = (selector) => rules.get(selector) || {};

test("bırakma çizgisi: iki sayfa arasında 2 px vurgu, tıklamayı engellemez", () => {
  const line = rule('.pdf-root .thumbnail[data-drop]::after');
  assert.equal(line.height, "2px");
  assert.equal(line.background, "var(--accent)");
  assert.equal(line.position, "absolute");
  assert.equal(line["pointer-events"], "none");
  // Çizgi sayfa aralığının (margin-bottom: --space-2) ortasında.
  assert.equal(rule(".pdf-root .thumbnail")["margin-bottom"], "var(--space-2)");
  assert.match(rule('.pdf-root .thumbnail[data-drop="before"]::after').top, /var\(--space-2\) \/ -2/);
  assert.match(rule('.pdf-root .thumbnail[data-drop="after"]::after').bottom, /var\(--space-2\) \/ -2/);
  // .current kendi işareti için ::before kullanıyor; çizgi onunla çakışmaz.
  assert.ok(rules.has(".pdf-root .thumbnail.current::before"));
});

test("sürükleme hareket üretmez: geçiş ya da animasyon yok", () => {
  for (const [selector, body] of rules) {
    if (!/data-drop|data-dragging|data-reorder/.test(selector)) continue;
    assert.equal(body.transition, undefined, `${selector} geçiş taşımamalı`);
    assert.equal(body.animation, undefined, `${selector} animasyon taşımamalı`);
  }
  assert.equal(rule(".pdf-root .thumbnail[data-dragging]").opacity, "0.5");
  // Sürüklerken metin seçilmez; tutamak imleci yalnız Sırala'da.
  assert.equal(rule(".pdf-root .thumbnail-list[data-reorder] .thumbnail")["user-select"], "none");
  assert.equal(rule(".pdf-root .thumbnail-list[data-reorder] .thumbnail-image-button").cursor, "grab");
});

test("dar şeritte ↑/↓ çifti birlikte kırılır, Seç kutusundan ayrılır", () => {
  assert.equal(rule(".pdf-root .thumbnail-move")["flex-wrap"], "wrap");
  assert.equal(rule(".pdf-root .thumbnail-step").flex, "none");
});

test("kullanılamayan taşıma düğmesi sönük görünür ama odakta kalır", () => {
  // Düğmeler `disabled` değil `aria-disabled` taşır (odak kaybolmasın diye);
  // görünüş, paylaşılan `.btn:disabled` ile aynı kalmalı ve üzerine gelince değişmemeli.
  const shared = readFileSync("src/shared-ui/shell.css", "utf8");
  const disabled = /\.btn:disabled \{([^}]*)\}/.exec(shared)[1];
  const off = rule('.pdf-root .reorder-controls .btn[aria-disabled="true"]');
  const hover = rule('.pdf-root .reorder-controls .btn[aria-disabled="true"]:hover');
  for (const prop of ["color", "background"]) {
    assert.match(disabled, new RegExp(`${prop}: ${off[prop].replace(/[()]/g, "\\$&")};`));
    assert.equal(hover[prop], off[prop]);
  }
  assert.equal(off.cursor, "default");
});

test("kenardaki ↑/↓ oku sönük görünür ama odakta kalır", () => {
  const shared = readFileSync("src/shared-ui/shell.css", "utf8");
  const disabled = /\.icon-btn:disabled \{([^}]*)\}/.exec(shared)[1];
  const off = rule('.pdf-root .thumbnail-step .icon-btn[aria-disabled="true"]');
  assert.match(disabled, new RegExp(`color: ${off.color.replace(/[()]/g, "\\$&")};`));
  assert.equal(rule('.pdf-root .thumbnail-step .icon-btn[aria-disabled="true"]:hover').background, "none");
  assert.equal(off.cursor, "default");
});

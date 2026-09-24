// Platforma göre pencere başlığı ve kısayol simgesi (saha maddesi 1, §12 ailesi).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { detectPlatform, shortcutLabel } from "./platform";

const here = dirname(fileURLToPath(import.meta.url));
const tokens = readFileSync(resolve(here, "../shared-ui/tokens.css"), "utf8");
const shell = readFileSync(resolve(here, "../shared-ui/shell.css"), "utf8");

// Gerçek kullanıcı aracıları: WebView2 153 (Windows 11) ve WKWebView (macOS).
const WEBVIEW2 =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36 Edg/153.0.4234.48";
const WKWEBVIEW = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";

describe("platform tespiti", () => {
  it("WebView2 Windows, WKWebView macOS, bilinmeyen 'other'", () => {
    expect(detectPlatform(WEBVIEW2)).toBe("windows");
    expect(detectPlatform(WKWEBVIEW)).toBe("mac");
    expect(detectPlatform("Node.js/22")).toBe("other");
  });

  it("kısayol simgesi: macOS ⌘, Windows Ctrl+", () => {
    expect(shortcutLabel("O", "mac")).toBe("⌘O");
    expect(shortcutLabel("O", "windows")).toBe("Ctrl+O");
    expect(shortcutLabel(",", "windows")).toBe("Ctrl+,");
  });
});

/** `selector { ... }` bloğunun gövdesi. */
function body(css: string, selector: string): string {
  const at = css.indexOf(selector);
  if (at < 0) return "";
  const open = css.indexOf("{", at);
  return css.slice(open + 1, css.indexOf("}", open));
}

describe("trafik ışığı bandı yalnız macOS'ta (madde 1)", () => {
  it("varsayılan bant 0; overlay'de 40 px", () => {
    expect(/--titlebar-h:\s*0px;/.test(body(tokens, ":root {"))).toBe(true);
    expect(/--titlebar-h:\s*40px;/.test(body(tokens, ':root[data-titlebar="overlay"]'))).toBe(true);
  });

  it("Windows'ta ürün satırı bardaki metinle aynı eksende: bar yüksekliği, üst boşluk yok", () => {
    const native = body(shell, ':root[data-titlebar="native"] .sidebar-head');
    expect(native).toMatch(/min-height:\s*var\(--toolbar-h\)/);
    expect(native).toMatch(/padding:\s*0 8px/);
  });

  it("tam ekran daraltması yalnız overlay'de; kök ilk çizimden önce işaretlenir", () => {
    expect(shell).toContain(':root[data-titlebar="overlay"] .shell[data-fullscreen="true"] { --titlebar-h: 10px; }');
    const main = readFileSync(resolve(here, "../main.tsx"), "utf8");
    expect(main.indexOf("applyPlatform(document.documentElement)")).toBeGreaterThan(-1);
    expect(main.indexOf("applyPlatform(document.documentElement)")).toBeLessThan(main.indexOf("ReactDOM.createRoot"));
  });
});

describe("kullanıcıya görünen metinde sabit ⌘ yok", () => {
  it("ipuçları ve duyurular kısayolu platforma göre yazar", () => {
    for (const file of ["Sidebar.tsx", "Settings.tsx", "../features/DocumentSurface.tsx"]) {
      const code = readFileSync(resolve(here, file), "utf8")
        .split("\n")
        .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
        .join("\n");
      expect(code, file).not.toMatch(/["'`][^"'`]*⌘[^"'`]*["'`]/);
    }
  });
});

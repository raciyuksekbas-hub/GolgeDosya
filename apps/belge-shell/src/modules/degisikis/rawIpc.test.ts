import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Belge baytları IPC'yi HAM geçer.
 *
 * Üç komut da eskiden baytları JSON sayı dizisi olarak taşıyordu: 20 MiB'lık
 * bir belge 71 MiB metne, ~1,1 sn ayrıştırmaya ve 425 MiB belleğe dönüşüyordu.
 * Okuma ve `.doc` dönüşümü artık `ArrayBuffer` döner; dönüşüm girdisi ve rapor
 * içeriği istek gövdesinde `Uint8Array` olarak gider.
 */

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const here = dirname(fileURLToPath(import.meta.url));

beforeEach(() => {
  invoke.mockReset();
});

const sample = () => Uint8Array.from({ length: 300 }, (_, index) => index % 256);

describe("belge okuma", () => {
  it("yanıt ArrayBuffer'dır ve File'a bayt bayt sarılır", async () => {
    const { fileFromPath } = await import("./CompareWorkspace");
    const bytes = sample();
    invoke.mockResolvedValueOnce(bytes.buffer);
    const file = await fileFromPath("/Users/x/Belgeler/sözleşme v2.docx");
    expect(invoke).toHaveBeenCalledWith("degisikis_read_document", { path: "/Users/x/Belgeler/sözleşme v2.docx" });
    expect(file.name).toBe("sözleşme v2.docx");
    expect(new Uint8Array(await file.arrayBuffer())).toEqual(bytes);
  });
});

describe("eski .doc dönüşümü", () => {
  it("girdi ham gövde, çıktı ArrayBuffer; sayı dizisi hiçbir yönde yok", async () => {
    const { convertLegacyDocLocally } = await import("./core/extractors");
    const input = sample();
    const output = Uint8Array.from([0x50, 0x4b, 0x03, 0x04, 9, 8, 7]);
    invoke.mockResolvedValueOnce(output.buffer);
    const converted = await convertLegacyDocLocally(input.buffer);
    const [command, payload, options] = invoke.mock.calls[0];
    expect(command).toBe("degisikis_convert_legacy_doc");
    expect(payload).toBeInstanceOf(Uint8Array);
    expect(payload).toEqual(input);
    expect(options).toBeUndefined();
    expect(new Uint8Array(converted)).toEqual(output);
  });

  it("Rust hata kodu kullanıcı diline çevrilir", async () => {
    const { convertLegacyDocLocally } = await import("./core/extractors");
    invoke.mockRejectedValueOnce("DOC_PASSWORD");
    await expect(convertLegacyDocLocally(sample().buffer)).rejects.toThrow("Parola korumalı");
  });
});

describe("rapor kaydetme", () => {
  it("içerik ham gövdede, ad yüzde-kodlu başlıkta gider", async () => {
    const { saveReport, REPORT_NAME_HEADER } = await import("./report");
    const body = new TextEncoder().encode("<html>özet</html>");
    invoke.mockResolvedValueOnce("/Users/x/Downloads/GolgeDosya-Karsilastirma-1.html");
    const saved = await saveReport("GolgeDosya-Karsilastirma-1.html", body, "html");
    expect(saved).toEqual({ path: "/Users/x/Downloads/GolgeDosya-Karsilastirma-1.html" });
    const [command, payload, options] = invoke.mock.calls[0];
    expect(command).toBe("degisikis_save_report");
    expect(payload).toBe(body);
    expect(options).toEqual({ headers: { [REPORT_NAME_HEADER]: "GolgeDosya-Karsilastirma-1.html" } });
  });

  it("ASCII dışı ad başlıkta yüzde-kodlanır (başlık yalnız ASCII taşır)", async () => {
    const { saveReport, REPORT_NAME_HEADER } = await import("./report");
    invoke.mockResolvedValueOnce("/x");
    await saveReport("Rapor özeti.html", new Uint8Array([1]), "html");
    const header = invoke.mock.calls[0][2].headers[REPORT_NAME_HEADER] as string;
    expect(header).toBe("Rapor%20%C3%B6zeti.html");
    expect(/^[\x21-\x7e]*$/u.test(header)).toBe(true);
    expect(decodeURIComponent(header)).toBe("Rapor özeti.html");
  });

  it("başlık adı Rust tarafıyla aynı", async () => {
    const { REPORT_NAME_HEADER } = await import("./report");
    const rust = readFileSync(resolve(here, "../../../src-tauri/src/modules/degisikis.rs"), "utf8");
    expect(rust).toContain(`const REPORT_NAME_HEADER: &str = "${REPORT_NAME_HEADER}";`);
    // Yazma ana iş parçacığında yapılmaz: komut eşzamansız, yazma spawn_blocking'te.
    expect(rust).toMatch(/pub async fn degisikis_save_report\(/);
    expect(rust).toMatch(/spawn_blocking\(move \|\| write_report\(/);
  });
});

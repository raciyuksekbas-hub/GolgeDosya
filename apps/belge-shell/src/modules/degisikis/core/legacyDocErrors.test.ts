import { describe, expect, it } from "vitest";
import { legacyDocErrorMessage } from "./extractors";

describe("Windows eski Word DOC hata mesajları", () => {
  it("Microsoft Word sağlayıcısı yoksa çözümü açıklar", () => {
    expect(legacyDocErrorMessage("DOC_WINDOWS_WORD_REQUIRED")).toBe(
      "DOC belgesi açılamadı. Windows'ta eski .doc belgelerinin dönüştürülmesi için Microsoft Word gereklidir. Belgeyi .docx olarak kaydedip yeniden deneyebilirsiniz.",
    );
  });

  it("bozuk ve parola korumalı DOC hatalarını ayırır", () => {
    expect(legacyDocErrorMessage("DOC_INVALID")).toBe("Belge geçerli bir Word .doc dosyası olarak okunamadı.");
    expect(legacyDocErrorMessage("DOC_PASSWORD")).toBe("Parola korumalı .doc belgeleri desteklenmiyor.");
  });
});

import { describe, expect, it } from "vitest";
import { relativeTime } from "./relativeTime";

// Rust `opened_at` saniye gönderir (settings.rs `as_secs`). Sunum ms varsayarsa
// her son kullanılan "1 Oca" olur. Bu test o kusuru kilitler.
describe("relativeTime", () => {
  const now = Date.UTC(2026, 8, 10, 12, 0, 0);
  const sec = (ms: number) => Math.floor((now - ms) / 1000);

  it("saniye alır, milisaniye değil", () => {
    expect(relativeTime(sec(15 * 60_000), now)).toBe("15 dk önce");
    expect(relativeTime(sec(3 * 3_600_000), now)).toBe("3 sa önce");
  });

  it("basamaklar: az önce · dk · sa · dün · gün · tarih", () => {
    expect(relativeTime(sec(10_000), now)).toBe("az önce");
    expect(relativeTime(sec(30 * 3_600_000), now)).toBe("dün");
    expect(relativeTime(sec(3 * 86_400_000), now)).toBe("3 gün önce");
    expect(relativeTime(sec(20 * 86_400_000), now)).toMatch(/Ağu/);
  });

  it("1970 tuzağı: bugünün saniyesi asla tarih olarak düşmez", () => {
    expect(relativeTime(Math.floor(now / 1000) - 60, now)).toBe("1 dk önce");
  });
});

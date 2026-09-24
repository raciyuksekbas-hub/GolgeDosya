// Kaybettiren eylemler (§54): sessiz kayıp yok, kayıpsız eylem sorulmaz.
import { describe, expect, it } from "vitest";
import { leaveCopy, leaveLoses, type LeaveAction } from "./leaveGuard";

const clean = { mode: "duzenek", dirty: {}, annexUnsaved: false };
const dirtyEdit = { mode: "duzenek", dirty: { duzenek: true }, annexUnsaved: false };

describe("ne zaman sorulur", () => {
  it("Düzenle'de kaydedilmemiş düzenleme varken kip değiştirmek, Kapat, işaret ve belge değiştirmek sorar", () => {
    for (const action of ["navigate", "close", "home", "replace"] as LeaveAction[])
      expect(leaveLoses(action, dirtyEdit, "/karsilastir"), action).toBe(true);
  });

  it("kayıpsız eylem sorulmaz: temiz Düzenle, aynı kipe tıklamak, Ekler", () => {
    for (const action of ["navigate", "close", "home", "replace"] as LeaveAction[])
      expect(leaveLoses(action, clean, "/karsilastir"), action).toBe(false);
    expect(leaveLoses("navigate", dirtyEdit, "duzenek")).toBe(false);
    // Ekler'in düzeni kabukta yaşar: gezinme ona dokunmaz.
    expect(leaveLoses("navigate", { mode: "ekler", dirty: {}, annexUnsaved: true }, "/duzenle")).toBe(false);
  });

  it("pencere kapanırken HERHANGİ bir kipte kaydedilmemiş iş varsa sorar", () => {
    expect(leaveLoses("window", { mode: "tavzih", dirty: {}, annexUnsaved: true })).toBe(true);
    expect(leaveLoses("window", { mode: "tavzih", dirty: { duzenek: true }, annexUnsaved: false })).toBe(true);
    expect(leaveLoses("window", { mode: "tavzih", dirty: {}, annexUnsaved: false })).toBe(false);
  });

  it("metin ne kaybolacağını ve neyin kalacağını söyler", () => {
    expect(leaveCopy("close").title).toBe("Mevcut çalışma temizlenecek");
    expect(leaveCopy("close").body).toMatch(/Devam edilsin mi\?$/);
    expect(leaveCopy("window").body).toContain("diskte kalır");
  });
});

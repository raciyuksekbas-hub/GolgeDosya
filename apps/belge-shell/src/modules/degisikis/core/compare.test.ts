import { describe, expect, it } from "vitest";
import { compareDocuments } from "./compare";
import { makeBlocks, normalizeTechnicalNoise } from "./normalize";
import { wordDiff } from "./wordDiff";

describe("Değişikİş karşılaştırma motoru", () => {
  it("değişen sayı ile eklenen ifadeyi kelime düzeyinde bulur", () => {
    const base = makeBlocks([{ text: "Madde 7 İşveren işçiye 30 gün önceden bildirimde bulunur." }], "base");
    const revised = makeBlocks([{ text: "Madde 7 İşveren işçiye 60 gün önceden yazılı olarak bildirimde bulunur." }], "revised");
    const result = compareDocuments(base, revised);

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].kind).toBe("modified");
    expect(result.changes[0].location).toBe("Madde 7");
    expect(result.rows[0].baseFragments).toContainEqual({ kind: "removed", text: "30" });
    expect(result.rows[0].revisedFragments).toContainEqual({ kind: "added", text: "60" });
    expect(result.rows[0].revisedFragments).toContainEqual({ kind: "added", text: "yazılı olarak" });
  });

  it.each([
    ["Bedel 20.000 TL olarak ödenir.", "Bedel 25.000 TL olarak ödenir.", "“20.000 TL” → “25.000 TL”"],
    ["Süre iki yıldır.", "Süre üç yıldır.", "“iki” → “üç”"],
    ["Tarih 01/09/2026 olarak belirlenmiştir.", "Tarih 01.09.2026 olarak belirlenmiştir.", "“01/09/2026” → “01.09.2026”"],
    ["Tutar 23.400 olarak yazılmıştır.", "Tutar 23.400,00 olarak yazılmıştır.", "“23.400” → “23.400,00”"],
  ])("güvenilir birebir replacement üretir: %s", (before, after, summary) => {
    const result = compareDocuments(
      makeBlocks([{ text: before }], "base"),
      makeBlocks([{ text: after }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].summary).toContain(summary);
  });

  it.each([
    ["Karar 2021 sayılıdır.", "Karar 3/2 sayılıdır."],
    ["Hüküm 3/1 bendindedir.", "Hüküm 15.08.2026 tarihindedir."],
    ["Süre 3 gündür.", "Süre 6325 gündür."],
  ])("güvensiz değerleri replacement olarak zorlamaz: %s", (before, after) => {
    const result = compareDocuments(
      makeBlocks([{ text: before }], "base"),
      makeBlocks([{ text: after }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].summary.some((item) => item.includes("→"))).toBe(false);
    expect(result.changes[0].summary.some((item) => item.startsWith("−"))).toBe(true);
    expect(result.changes[0].summary.some((item) => item.startsWith("+"))).toBe(true);
  });

  it("madde numarasını gövdeden ayrı özetler", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Madde 3 Aynı hüküm korunur." }], "base"),
      makeBlocks([{ text: "Madde 5 Aynı hüküm korunur." }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].summary).toEqual(["Madde numarası: 3 → 5"]);
    expect(result.changes[0].location).toBe("Madde 5");
  });

  it("tamamen silinen madde başlığı ve paragraflarını tek kullanıcı değişikliği yapar", () => {
    const base = makeBlocks([
      { text: "MADDE 10 - ÖNCEKİ HÜKÜM" },
      { text: "Önceki hüküm korunur." },
      { text: "MADDE 11 - BELGELERİN SAKLANMASI" },
      { text: "Belgeler on yıl süreyle saklanır." },
      { text: "Arşiv güvenli biçimde tutulur." },
      { text: "MADDE 12 - SON HÜKÜM" },
      { text: "Son hüküm korunur." },
    ], "base");
    const revised = makeBlocks([
      { text: "MADDE 10 - ÖNCEKİ HÜKÜM" },
      { text: "Önceki hüküm korunur." },
      { text: "MADDE 12 - SON HÜKÜM" },
      { text: "Son hüküm korunur." },
    ], "revised");
    const result = compareDocuments(base, revised);

    expect(result.rawChanges).toHaveLength(3);
    expect(result.rawChanges.every((change) => change.kind === "removed")).toBe(true);
    expect(result.changes).toHaveLength(1);
    expect(result.changes[0]).toMatchObject({
      kind: "removed",
      location: "Madde 11 — Belgelerin Saklanması",
      summary: ["Madde bütünüyle silindi.", "2 içerik paragrafı"],
      rowIndices: [2, 3, 4],
    });
  });

  it("tamamen eklenen madde başlığı ve paragraflarını tek kullanıcı değişikliği yapar", () => {
    const base = makeBlocks([
      { text: "MADDE 12 - SON HÜKÜM" },
      { text: "Son hüküm korunur." },
    ], "base");
    const revised = makeBlocks([
      { text: "MADDE 13 - VERİLERİN YEDEKLENMESİ" },
      { text: "Veriler her gün yedeklenir." },
      { text: "Yedekler güvenli yerde tutulur." },
      { text: "MADDE 12 - SON HÜKÜM" },
      { text: "Son hüküm korunur." },
    ], "revised");
    const result = compareDocuments(base, revised);

    expect(result.rawChanges).toHaveLength(3);
    expect(result.changes).toHaveLength(1);
    expect(result.changes[0]).toMatchObject({
      kind: "added",
      location: "Madde 13 — Verilerin Yedeklenmesi",
      summary: ["Madde bütünüyle eklendi.", "2 içerik paragrafı"],
      rowIndices: [0, 1, 2],
    });
  });

  it("esaslı hüküm yeniden yazımını silinen ve eklenen metinle özetler", () => {
    const before = "Madde 7 Eser üzerindeki mali haklar, ilgili çalışmaya ilişkin ücretin tam ve eksiksiz olarak ödenmesiyle Hizmet Alana devredilir.";
    const after = "Madde 7 Eser üzerindeki mali haklar, ilgili çalışmanın teslimi ve bedelinin ödenmesi üzerine Hizmet Alana 5 yıl süreyle münhasır kullanım lisansı olarak verilir.";
    const result = compareDocuments(
      makeBlocks([{ text: before }], "base"),
      makeBlocks([{ text: after }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].summary.length).toBeGreaterThanOrEqual(2);
    expect(result.changes[0].summary.some((line) => line.startsWith("− "))).toBe(true);
    expect(result.changes[0].summary.some((line) => line.startsWith("+ "))).toBe(true);
    expect(result.changes[0].summary.join(" ")).not.toContain("→");
    expect(result.changes[0].summary.join(" ")).toContain("münhasır kullanım lisansı");
  });

  it.each([
    ["Ödeme 60 (Altmış) takvim günü içinde yapılır.", "Ödeme 3 (Üç) iş günü içinde yapılır.", "“60 (Altmış) takvim günü” → “3 (Üç) iş günü”"],
    ["Personel en az 2 (iki) ay önce ayrılmış olmalıdır.", "Personel en az 10 (on) yıl önce ayrılmış olmalıdır.", "“2 (iki) ay” → “10 (on) yıl”"],
  ])("süre replacement'ını sayı, yazı ve birimiyle birlikte özetler", (before, after, summary) => {
    const result = compareDocuments(
      makeBlocks([{ text: before }], "base"),
      makeBlocks([{ text: after }], "revised"),
    );

    expect(result.changes[0].summary[0]).toBe(summary);
  });

  it("esaslı uzun yeniden yazımda kritik süre replacement'ını özetin başında korur", () => {
    const before = "Madde 17 SATICI, ALICI personelini ancak görevinden en az 2 (iki) ay önce ayrılmış olması halinde çalıştırabilir; diğer yükümlülükler aynen devam eder.";
    const after = "Madde 17 SATICI, ALICI personelini ancak görevinden en az 10 (on) yıl önce ayrılmış olması halinde çalıştırabilir; doğrudan ve dolaylı zararları karşılar, her personel için cezai şart öder ve rekabet yasağına uyar.";
    const result = compareDocuments(
      makeBlocks([{ text: before }], "base"),
      makeBlocks([{ text: after }], "revised"),
    );

    expect(result.changes[0].summary[0]).toBe("“2 (iki) ay” → “10 (on) yıl”");
    expect(result.changes[0].summary.some((line) => /değişiklik daha$/u.test(line))).toBe(true);
  });

  it("bir paragrafın iki paragrafa bölünmesini içerik değişikliği saymaz", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Birinci cümle. İkinci cümle. Üçüncü cümle." }], "base"),
      makeBlocks([{ text: "Birinci cümle." }, { text: "İkinci cümle. Üçüncü cümle." }], "revised"),
    );

    expect(result.rows).toHaveLength(1);
    expect(result.changes).toHaveLength(0);
  });

  it("iki paragrafın tek paragrafta birleşmesini içerik değişikliği saymaz", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Birinci cümle." }, { text: "İkinci cümle. Üçüncü cümle." }], "base"),
      makeBlocks([{ text: "Birinci cümle. İkinci cümle. Üçüncü cümle." }], "revised"),
    );

    expect(result.rows).toHaveLength(1);
    expect(result.changes).toHaveLength(0);
  });

  it("iki tarafta kayan paragraf sınırını içerik değişikliği saymaz", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Birinci cümle. İkinci cümle." }, { text: "Üçüncü cümle. Dördüncü cümle." }], "base"),
      makeBlocks([{ text: "Birinci cümle." }, { text: "İkinci cümle. Üçüncü cümle. Dördüncü cümle." }], "revised"),
    );

    expect(result.rows).toHaveLength(1);
    expect(result.changes).toHaveLength(0);
  });

  it("paragraf bölünürken eklenen yalnız yeni cümleyi gösterir", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Birinci cümle. İkinci cümle. Üçüncü cümle." }], "base"),
      makeBlocks([{ text: "Birinci cümle. Yeni cümle." }, { text: "İkinci cümle. Üçüncü cümle." }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].kind).toBe("added");
    expect(result.changes[0].summary).toEqual(["+ “Yeni cümle.”"]);
  });

  it("paragraflar birleşirken yalnız kaldırılan cümleyi gösterir", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Birinci cümle. Silinecek cümle." }, { text: "İkinci cümle." }], "base"),
      makeBlocks([{ text: "Birinci cümle. İkinci cümle." }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].kind).toBe("removed");
    expect(result.changes[0].summary).toEqual(["− “Silinecek cümle.”"]);
  });

  it("paragraf birleşmesindeki kısa replacement güvenli kalır", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Bildirim süresi 30 gündür." }, { text: "Başvuru yazılı yapılır." }], "base"),
      makeBlocks([{ text: "Bildirim süresi 90 gündür. Başvuru yazılı yapılır." }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].kind).toBe("modified");
    expect(result.changes[0].summary).toEqual(["“30” → “90”"]);
  });

  it("Merve ihtarname split örüntüsünde yalnız üç maddi farkı gösterir", () => {
    const addedSentence = "Bu durumun müvekkilin, 14 Kasım tarihi itibarıyla şirketin genel durumuna ilişkin ve şirketin ilgili mevzuat ve yönetmeliklere aykırı faaliyetlerini açık biçimde ortaya koyduğu raporunu yönetim kuruluna sunduktan sonra, mali işler departmanı tarafından çalıştığı birimin kapatılacağı yönünde bir baskıyla karşı karşıya kalması akabinde yaşanması düşündürücüdür.";
    const removedOvertime = "nihayet fazla mesai yapmasına karşın bu mesailerinin karşılığı da ödenmemiştir.";
    const commonA = "Şirket nezdinde uzun yıllardır çalışan müvekkilin görevini özenle yerine getirdiği, tüm sorumluluklarını zamanında tamamladığı ve yönetim süreçlerine eksiksiz katkı sunduğu sabittir.";
    const commonBStart = "Buna rağmen çalışma koşulları ağırlaştırılmış ve ücretleri geç ödenmiştir;";
    const commonBEnd = "İş ilişkisi boyunca müvekkil tüm yazılı bildirimleri zamanında yapmış ve yöneticilerini bilgilendirmiştir.";
    const result = compareDocuments(
      makeBlocks([
        { text: `${commonA} ${commonBStart} ${removedOvertime} ${commonBEnd}` },
        { text: "Müvekkilin ücret, fazla mesai ücretlerini, yıllık izin ve sair işçilik alacaklarını talep ederiz." },
      ], "base"),
      makeBlocks([
        { text: `${commonA} ${addedSentence}` },
        { text: `${commonBStart} ${commonBEnd}` },
        { text: "Müvekkilin ücret, yıllık izin ve sair işçilik alacaklarını talep ederiz." },
      ], "revised"),
    );

    expect(result.changes).toHaveLength(3);
    expect(result.changes.map((change) => change.kind)).toEqual(["added", "removed", "removed"]);
    expect(result.changes[0].revisedText).toBe(addedSentence);
    expect(result.changes[1].baseText).toBe(removedOvertime);
    expect(result.changes[2].baseText).toBe("fazla mesai ücretlerini,");
    expect(result.rows.filter((row) => row.kind === "added" || row.kind === "removed")).toHaveLength(0);
  });

  it("tamamen eklenen maddeyi ayrı değişiklik sayar", () => {
    const base = makeBlocks([
      { text: "Madde 1 Amaç ve kapsam." },
      { text: "Madde 3 Son hükümler." },
    ], "base");
    const revised = makeBlocks([
      { text: "Madde 1 Amaç ve kapsam." },
      { text: "Madde 2 Yeni yükümlülük eklenmiştir." },
      { text: "Madde 3 Son hükümler." },
    ], "revised");
    const result = compareDocuments(base, revised);

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0]).toMatchObject({ kind: "added", location: "Madde 2" });
  });

  it("aynı başlıklı farklı numaralı Cezai Şart maddesini yapısal olarak eşler", () => {
    const result = compareDocuments(
      makeBlocks([
        { text: "Madde-10:Cezai Şartlar:" },
        { text: "İhlalde bulunan taraf 250.000 USD tutarında cezai şart öder." },
      ], "base"),
      makeBlocks([
        { text: "14- CEZAİ ŞART" },
        { text: "İhlalde bulunan taraf 100.000 USD tutarında cezai şart öder." },
      ], "revised"),
    );

    expect(result.rows.some((row) => row.base?.text.includes("250.000 USD") && row.revised?.text.includes("100.000 USD"))).toBe(true);
    expect(result.changes.some((change) => change.summary.includes("“250.000 USD” → “100.000 USD”"))).toBe(true);
  });

  it("ortak Rekabet Yasağı başlığında 3 yıl → 2 yıl değişikliğini eşler", () => {
    const result = compareDocuments(
      makeBlocks([
        { text: "Madde-7: Müşteri İlişkilerinin Korunması ve Rekabet Yasağı" },
        { text: "Sözleşmenin sona ermesinden itibaren 3 (üç) yıl süreyle geçerlidir." },
      ], "base"),
      makeBlocks([
        { text: "10- REKABET YASAĞI (NON-COMPETE)" },
        { text: "Sözleşmenin sona ermesinden itibaren 2 (iki) yıl süreyle geçerlidir." },
      ], "revised"),
    );

    expect(result.changes.some((change) => change.summary.includes("“3 (üç) yıl” → “2 (iki) yıl”"))).toBe(true);
  });

  it("kontrollü Uyuşmazlık başlık eşleşmesinde yetki daralmasını öne çıkarır", () => {
    const result = compareDocuments(
      makeBlocks([
        { text: "Madde-14: İhtilafların Çözümü" },
        { text: "Türk Hukuku uygulanacak olup İstanbul Anadolu Mahkemeleri ve İcra Daireleri münhasıran yetkilidir." },
      ], "base"),
      makeBlocks([
        { text: "13- UYUŞMAZLIK" },
        { text: "İstanbul Mahkemeleri ve İcra Daireleri yetkilidir." },
      ], "revised"),
    );

    expect(result.changes.some((change) => change.summary.includes("“İstanbul Anadolu” → “İstanbul”"))).toBe(true);
    expect(result.changes.some((change) => change.summary.join(" ").includes("Türk Hukuku"))).toBe(true);
    expect(result.changes.some((change) => change.summary.join(" ").includes("münhasıran"))).toBe(true);
  });

  it("düşük güvenli uzun hükümleri replacement'a zorlamaz", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Proje kapsamında sağlanan ham veriler ve müşteri verileri KODAR'ın münhasır mülkiyetindedir; çoğaltılamaz, devredilemez ve ticari amaçla kullanılamaz." }], "base"),
      makeBlocks([{ text: "İŞ ORTAĞI, KODAR’a ait hiçbir dokümanı, yazılımı veya veriyi sözleşme sona erdikten sonra saklayamaz, iade etmekle yükümlüdür." }], "revised"),
    );

    expect(result.changes.map((change) => change.kind).sort()).toEqual(["added", "removed"]);
    expect(result.rows.some((row) => row.kind === "modified")).toBe(false);
  });

  it("çok sayıdaki yakın mikro farkı tek sunum kartında toplar", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Taraf A kırmızı belgeyi, Taraf B mavi eki, Taraf C yeşil kaydı ve Taraf D sarı nüshayı kabul eder." }], "base"),
      makeBlocks([
        { text: "Taraf A kızıl belgeyi; Taraf B lacivert eki," },
        { text: "Taraf C yeşil kaydı ve Taraf D turuncu nüshayı kabul eder!" },
      ], "revised"),
    );

    expect(result.rows).toHaveLength(1);
    expect(result.rows[0].baseFragments.filter((fragment) => fragment.kind === "removed").length).toBeGreaterThanOrEqual(4);
    expect(result.changes).toHaveLength(1);
  });

  it("paragraf sınırı değişen birebir aynı tanım cümlesini fark saymaz", () => {
    const definition = "“Açıklayan Taraf”: Gizli Bilgiyi diğer Tarafa açıklayan Tarafı ifade eder.";
    const result = compareDocuments(
      makeBlocks([
        { text: `“Gizli Bilgi”: Paylaşılan gizli bilgi bütününü ifade eder. ${definition}` },
        { text: "“Alan Taraf”: Gizli Bilgiyi alan veya depolayan Tarafı ifade eder." },
      ], "base"),
      makeBlocks([
        { text: "“Gizli Bilgi”: Paylaşılan gizli bilgi bütününü ifade eder." },
        { text: `${definition} “Alan Taraf”: Gizli Bilgiyi alan veya depolayan Tarafı ifade eder.` },
      ], "revised"),
    );

    expect(result.changes).toHaveLength(0);
  });

  it("araya eklenen paragrafta exact komşuları unchanged tutar", () => {
    const blocks = ["Ortak A hükmü aynen korunur.", "Ortak B hükmü aynen korunur.", "Ortak C hükmü aynen korunur."];
    const result = compareDocuments(
      makeBlocks(blocks.map((text) => ({ text })), "base"),
      makeBlocks([...blocks.slice(0, 2), "Yeni sorumluluk tavanı paragrafı eklenmiştir.", blocks[2]].map((text) => ({ text })), "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].kind).toBe("added");
    expect(result.rows.some((row) => row.kind === "removed" || row.kind === "modified")).toBe(false);
    expect(result.rows.flatMap((row) => row.base?.id.split("+") ?? [])).toHaveLength(3);
  });

  it("paragraf sınırında taşınan exact hükmü delete + add üretmeden sabitler", () => {
    const relocated = "Aksi ilgili proje ekinde açıkça kararlaştırılmadıkça, Tarafların birbirine karşı sorumluluğu doğrudan ve dolaylı zararlarını kapsar.";
    const mitigation = "Taraflardan her biri zararın artmasını engellemek için makul önlemleri almakla yükümlüdür.";
    const result = compareDocuments(
      makeBlocks([
        { text: "Taraflar ihlal nedeniyle doğan ispatlanabilir zararları tazmin eder." },
        { text: `${relocated} ${mitigation}` },
      ], "base"),
      makeBlocks([
        { text: `Taraflar ihlal nedeniyle doğan doğrudan zararları tazmin eder. ${relocated}` },
        { text: mitigation },
        { text: "Toplam sorumluluk proje bedelinin %100'ünü aşamaz." },
      ], "revised"),
    );

    expect(result.changes.filter((change) => change.baseText?.includes(relocated) || change.revisedText?.includes(relocated))).toHaveLength(0);
    expect(result.changes.some((change) => change.kind === "added" && change.revisedText?.includes("%100"))).toBe(true);
  });

  it("uzun tek yönlü hüküm kaldırmasını ayrı silme kartı yapar", () => {
    const deleted = "ESC tarafından proje kapsamında geliştirilecek çıktıların hukuki rejimi ilgili proje ekinde açıkça belirlenecektir. Devredilecek Proje Teslimatı olarak tanımlanmayan hiçbir çıktı kendiliğinden KODAR'a devredilmiş sayılmaz. Aksi yazılı kararlaştırılmadıkça mali haklar ESC'de kalır ve KODAR'a bedelin ödenmesi şartıyla sınırsız süreli, dünya çapında, devredilemeyen ve münhasır olmayan kullanım lisansı verilir.";
    const common = "Tarafların önceden geliştirdiği yazılım ve know-how üzerindeki hakları kendilerine ait kalır.";
    const result = compareDocuments(
      makeBlocks([{ text: `${common} ${deleted}` }], "base"),
      makeBlocks([{ text: `${common} Taraflar çalışanlarının ihlallerinden doğan zararları tazmin eder.` }], "revised"),
    );

    const deletion = result.changes.find((change) => change.kind === "removed" && change.baseText?.includes("mali haklar ESC'de kalır"));
    expect(deletion).toBeDefined();
    expect(deletion?.baseText).toContain("Devredilecek Proje Teslimatı");
  });

  it("her kaynak structural child'ı sonuçta tam bir kez tüketir", () => {
    const base = makeBlocks([
      { text: "Madde 4 Fikrî Mülkiyet" },
      { text: "Ortak A paragrafı korunur." },
      { text: "Tamamen kaldırılan önemli ve uzun proje teslimatı hükmü burada yer alır." },
      { text: "Ortak B paragrafı korunur." },
    ], "base");
    const revised = makeBlocks([
      { text: "Madde 4 Fikrî Mülkiyet" },
      { text: "Ortak A paragrafı korunur." },
      { text: "Yeni paragraf eklenir." },
      { text: "Ortak B paragrafı korunur." },
    ], "revised");
    const result = compareDocuments(base, revised);
    const consumed = (side: "base" | "revised") => result.rows.flatMap((row) => row[side]?.id.split("+") ?? []);

    expect(consumed("base").sort()).toEqual(base.map((block) => block.id).sort());
    expect(consumed("revised").sort()).toEqual(revised.map((block) => block.id).sort());
    expect(result.changes.map((change) => change.kind).sort()).toEqual(["added", "removed"]);
  });

  it("kısa [•] Şubesi ifadesinin paragraf sınırı kaymasını fark saymaz", () => {
    const result = compareDocuments(
      makeBlocks([
        { text: "[•] BANK A.Ş. [•] Şubesi" },
        { text: "İsim ve Ünvan | İsim ve Ünvan" },
      ], "base"),
      makeBlocks([
        { text: "[•] BANK A.Ş." },
        { text: "[•] Şubesi İsim ve Ünvan | İsim ve Ünvan" },
      ], "revised"),
    );

    expect(result.changes).toHaveLength(0);
  });

  it("tek karakter artefaktı uzun hukukî hükümle replacement yapmaz", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "F" }], "base"),
      makeBlocks([{ text: "ALICI, kendisinin bu eylemleri nedeniyle SATICI'nın yazılım desteğini kullanamayacağını ve doğan zararları tazmin edeceğini kabul eder." }], "revised"),
    );

    expect(result.changes.map(({ kind }) => kind).sort()).toEqual(["added", "removed"]);
    expect(result.rows.some(({ kind }) => kind === "modified")).toBe(false);
  });

  it("kısa gerçek sayı replacement'ını korur", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Bildirim süresi 30 gündür." }], "base"),
      makeBlocks([{ text: "Bildirim süresi 90 gündür." }], "revised"),
    );
    expect(result.changes[0].summary).toContain("“30” → “90”");
  });

  it("ortak KEP label'ını eşleyip yalnız değeri silinen gösterir", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "KEP Adresi\tronesans.holding@hs03.kep.tr", kind: "table" }], "base"),
      makeBlocks([{ text: "KEP Adresi", kind: "table" }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].kind).toBe("removed");
    expect(result.changes[0].summary.join(" ")).toContain("ronesans.holding@hs03.kep.tr");
    expect(result.changes[0].summary.join(" ")).not.toContain("KEP Adresi");
  });

  it("ortak telefon label'ında 0532 515 1443 → 0 replacement'ını korur", () => {
    const result = compareDocuments(
      makeBlocks([{ text: "Tel / Faks | 0532 515 1443", kind: "table" }], "base"),
      makeBlocks([{ text: "Tel / Faks | 0", kind: "table" }], "revised"),
    );

    expect(result.changes).toHaveLength(1);
    expect(result.rows[0]).toMatchObject({ kind: "modified" });
    expect(result.changes[0].summary.join(" ")).toContain("0");
  });

  it("uzun kart özetinde her engine segmentini görünür veya collapsed olarak hesaplar", () => {
    const before = "Fikri Mülkiyet hükümleri ALICI'nın kendisi, müdürleri, çalışanları ve grup şirketleri bakımından geçerlidir; ALICI makul önlem alır.";
    const after = "Fikri Mülkiyet hükümleri ALICI'nın kendisi, müdürleri, temsilcileri, çalışanları, danışmanları, taşeronları, grup şirketleri ve ifa yardımcıları bakımından geçerlidir; ALICI gerekli teknik, idari ve hukukî önlemleri alır. ALICI doğrudan veya dolaylı zararı ve Sözleşme bedelinin 4 (Dört) katı tutarında cezai şartı öder; diğer hukukî haklar saklıdır.";
    const result = compareDocuments(makeBlocks([{ text: before }], "base"), makeBlocks([{ text: after }], "revised"));
    const change = result.changes[0];
    const accounting = change.presentation!;
    const accounted = new Set([...accounting.visibleSegmentIds, ...accounting.collapsedSegmentIds]);

    expect(change.summary.join(" ")).toContain("4 (Dört) katı tutarında cezai şart");
    expect(change.summary.some((line) => /^\+ \d+ değişiklik daha$/u.test(line))).toBe(true);
    expect(accounted.size).toBe(accounting.segmentCount);
    expect(accounting.visibleSegmentIds.every((id) => !accounting.collapsedSegmentIds.includes(id))).toBe(true);
  });

  it("noktalama ve büyük/küçük harf değişikliklerini korur", () => {
    const base = makeBlocks([{ text: "Taraflar kabul eder." }], "base");
    const revised = makeBlocks([{ text: "taraflar kabul eder!" }], "revised");
    const result = compareDocuments(base, revised);

    expect(result.changes).toHaveLength(1);
    expect(result.changes[0].kind).toBe("modified");
  });

  it("teknik satır sonu ve ardışık boşluk gürültüsünü normalleştirir", () => {
    expect(normalizeTechnicalNoise("yükümlü-\n lük   devam eder")).toBe("yükümlülük devam eder");
  });

  it("uzun metinlerde güvenli diff geri dönüşü kullanır", () => {
    const left = Array.from({ length: 450 }, (_, index) => `kelime${index}`).join(" ");
    const right = `${left} ek`;
    const diff = wordDiff(left, right);
    expect(diff.revised.at(-1)).toEqual({ kind: "added", text: "ek" });
  });

  it("sentetik 20 maddi değişikliğin tamamını kullanıcı değişikliği olarak korur", () => {
    const base = makeBlocks(Array.from({ length: 20 }, (_, index) => ({
      text: `Madde ${index + 1} Örnek yükümlülük bedeli ${100 + index}.000 TL'dir.`,
    })), "base");
    const revised = makeBlocks(Array.from({ length: 20 }, (_, index) => ({
      text: `Madde ${index + 1} Örnek yükümlülük bedeli ${200 + index}.000 TL'dir.`,
    })), "revised");

    const result = compareDocuments(base, revised);

    expect(result.changes).toHaveLength(20);
    expect(result.changes.every((change) => change.kind === "modified")).toBe(true);
  });
});

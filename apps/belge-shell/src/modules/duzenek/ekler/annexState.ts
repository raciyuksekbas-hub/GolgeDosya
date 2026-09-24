/**
 * Ek atama ve düzenleme — SAF dönüşümler.
 *
 * Bağımsız DüzenEk'te de bu işlemler TypeScript durum dönüşümleriydi; Rust
 * yalnız bitmiş `Project`'i plan/dışa aktarma anında alır. Semantik oradan
 * taşındı, mantık yeniden yazılmadı.
 */
import type { AnnexSource, LogicalExhibit, PreparedAnnex, Project } from "./types";

/** Bir kaynağa atanmış ek — yoksa `undefined`. */
export function assignmentOf(project: Project, sourceId: string): LogicalExhibit | undefined {
  return project.exhibits.find((e) => e.sources.some((s) => s.source_id === sourceId));
}

/** Kullanıcıya gösterilen atama durumu: "Atanmadı" ya da "Ek-3". */
export function assignmentLabel(project: Project, sourceId: string): string {
  const exhibit = assignmentOf(project, sourceId);
  return exhibit ? `Ek-${exhibit.order}` : "Atanmadı";
}

/** Sıra numaralarını 1..n olarak yeniden kur. */
function renumber(exhibits: LogicalExhibit[]): LogicalExhibit[] {
  return exhibits.map((e, index) => ({ ...e, order: index + 1 }));
}

/**
 * Kaynağı bir eke ata.
 *
 * Bir kaynak AYNI ANDA yalnız bir ekte durur: başka bir ekteyse oradan
 * çıkarılır. Aksi hâlde aynı belge nihai pakette iki kez basılırdı.
 */
export function assignSource(project: Project, sourceId: string, exhibitId: string): Project {
  const exhibits = project.exhibits.map((e) => {
    const without = e.sources.filter((s) => s.source_id !== sourceId);
    if (e.id !== exhibitId) return { ...e, sources: without };
    return { ...e, sources: [...without, { source_id: sourceId }] };
  });
  return { ...project, exhibits };
}

/** Kaynağın atamasını kaldır: yeniden "Atanmadı" olur. */
export function unassignSource(project: Project, sourceId: string): Project {
  return {
    ...project,
    exhibits: project.exhibits.map((e) => ({
      ...e,
      sources: e.sources.filter((s) => s.source_id !== sourceId),
    })),
  };
}

export function createExhibit(project: Project, name: string, id: string): Project {
  const order = project.exhibits.length + 1;
  return { ...project, exhibits: [...project.exhibits, { id, order, name, sources: [] }] };
}

export function renameExhibit(project: Project, exhibitId: string, name: string): Project {
  return {
    ...project,
    exhibits: project.exhibits.map((e) => (e.id === exhibitId ? { ...e, name } : e)),
  };
}

/** Eki sil; içindeki kaynaklar "Atanmadı"ya döner (silinmez). */
export function removeExhibit(project: Project, exhibitId: string): Project {
  return { ...project, exhibits: renumber(project.exhibits.filter((e) => e.id !== exhibitId)) };
}

export function moveExhibit(project: Project, index: number, direction: -1 | 1): Project {
  const target = index + direction;
  if (target < 0 || target >= project.exhibits.length) return project;
  const exhibits = [...project.exhibits];
  [exhibits[index], exhibits[target]] = [exhibits[target], exhibits[index]];
  return { ...project, exhibits: renumber(exhibits) };
}

/** Henüz hiçbir eke atanmamış kaynaklar. */
export function unassignedSources(project: Project): AnnexSource[] {
  const assigned = new Set(project.exhibits.flatMap((e) => e.sources.map((s) => s.source_id)));
  return project.sources.filter((s) => !assigned.has(s.id));
}

/** Dosya adından uzantıyı at: ek adının makul başlangıcı. */
export function nameFromFile(fileName: string): string {
  return fileName.replace(/\.[^/.]+$/u, "");
}

/**
 * Otomatik Dağıt — YALNIZ atanmamış belgeleri sıradaki eklere dağıtır.
 *
 * Kullanıcının elle yaptığı atamalar KORUNUR. Bağımsız DüzenEk'in semantiği
 * budur ve saha turunda açıkça korunması istendi: kullanıcı A'yı Ek-1'e,
 * C'yi Ek-3'e koyduysa dağıtım onlara dokunmaz, yalnız B ve D için yeni ek
 * açar.
 *
 * `newId` dışarıdan verilir: saf fonksiyon kendi kimliğini üretmez.
 */
export function autoDistribute(project: Project, newId: (index: number) => string): Project {
  const pending = unassignedSources(project);
  if (pending.length === 0) return project;
  const created = pending.map((source, index) => ({
    id: newId(index),
    order: project.exhibits.length + index + 1,
    name: nameFromFile(source.file_name),
    sources: [{ source_id: source.id }],
  }));
  return { ...project, exhibits: [...project.exhibits, ...created] };
}

/** Bir ekin toplam sayfa sayısı (kaynakların sayfalarının toplamı). */
export function exhibitPageCount(project: Project, exhibit: LogicalExhibit): number {
  return exhibit.sources.reduce((total, ref) => {
    const source = project.sources.find((s) => s.id === ref.source_id);
    return total + (source?.page_count ?? 0);
  }, 0);
}

/**
 * Dilekçe sonuna konacak EKLER listesi.
 *
 * Biçim `ekler_core::exhibits_list` ile aynıdır; motor aynı listeyi paket
 * içine de yazar, iki yerde iki ayrı biçim olmaz.
 */
export function exhibitsListText(project: Project): string {
  const lines = project.exhibits.map(
    (e) => `Ek-${e.order}: ${e.name} — ${exhibitPageCount(project, e)} sayfa`,
  );
  return `EKLER\n\n${lines.join("\n")}\n`;
}

/**
 * Ekler işleminin yaşam döngüsü (§53):
 *
 *   empty     → hiç belge ve ek yok
 *   working   → düzen kuruluyor; henüz hazırlanamaz (ek yok ya da boş ek var)
 *   ready     → her ekte en az bir belge var; "Ekleri Hazırla ve Kaydet" açık
 *   completed → bu düzen hazırlandı ve diske yazıldı
 *
 * `completed`, hazırlanan düzenle ŞİMDİKİ düzen aynı nesne olduğunda doğrudur.
 * Düzen değişmeden yeni nesne üretilmez (bütün dönüşümler değişmezdir); bir
 * değişiklik yapıldığında düzen yeniden `working`/`ready` olur ve paket bu
 * değişikliği içermez.
 *
 * Saha (madde 19): "İşlemden sonra çıkış yapamıyorum. Yeni bir Ekler işlemine
 * başlayamıyorum." Yaşam döngüsü yoktu: hazırlamadan sonra ekran, hazırlamadan
 * önceki ekranın aynısıydı ve düzeni temizlemenin tek yolu her belgeyi ve eki
 * tek tek silmekti.
 */
export type AnnexPhase = "empty" | "working" | "ready" | "completed";

/** Hazırlamayı engelleyen boş ekler. Motor bunları reddeder ("Her Ek en az bir belge içermeli"). */
export function emptyExhibits(project: Project): LogicalExhibit[] {
  return project.exhibits.filter((e) => e.sources.length === 0);
}

export function annexPhase(project: Project, prepared: PreparedAnnex | null): AnnexPhase {
  if (prepared && prepared.snapshot === project) return "completed";
  if (project.sources.length === 0 && project.exhibits.length === 0) return "empty";
  return project.exhibits.length > 0 && emptyExhibits(project).length === 0 ? "ready" : "working";
}

/**
 * "Yeni Ekler İşlemi" bir şey kaybettirir mi?
 *
 * Tamamlanmış paket diskte duruyor; boş düzen zaten boş. Kaybolacak olan,
 * kurulan ama hazırlanmamış (ya da hazırlandıktan sonra değiştirilmiş) düzendir.
 */
export function discardsWork(project: Project, prepared: PreparedAnnex | null): boolean {
  const phase = annexPhase(project, prepared);
  return phase === "working" || phase === "ready";
}

/**
 * Sağ panelin açık mı kapalı mı geleceği — saf karar.
 *
 * Panel iki türdür:
 *
 * - "details" (Karşılaştır, Denetle): ikincil bağlam. Pencere 1280 px'in
 *   altındayken varsayılan kapalı gelir; bu kural `Layout` içinde, değişmeden
 *   durur.
 * - "tools" (Düzenle): kipin araçlarının, birleştirilecek belge listesinin ve
 *   imza onayının TEK yeri. Pencere genişliğine değil, panel açıldığında
 *   workspace'te KALAN alana bakar: o alan en az `TOOLS_MIN_WORKSPACE_W`
 *   kalıyorsa açık gelir, kalmıyorsa kapalı. Kullanıcı düğmeyle açar ya da
 *   kapatırsa onun seçimi geçerlidir; kipten ayrılınca kural yeniden işler.
 *
 * Eskiden iki tür de aynı 1280 eşiğine bağlıydı: 1279 px'lik bir pencerede
 * Düzenle'nin yedi aracı ve imza onayı tamamen görünmez oluyordu, araçlar
 * yalnız tam ekranda ya da büyütülmüş pencerede geri geliyordu.
 */

export type InspectorKind = "details" | "tools";

/** Panelin kapladığı sütun: `--inspector-w` (300) + `--inspector-inset` (12). */
export const INSPECTOR_SLOT_W = 312;

/**
 * Araçlar paneli açıkken workspace'te (sayfa şeridi + belge alanı) en az bu
 * kadar kalmalı. 560 px'te belge alanı ~456 px olur: sayfa okunur, şerit tek
 * satırda kalır. Daha darında panel önizlemeyi kullanılmaz hâle getiriyordu
 * (ör. 900 px, %150 metin: sayfa kutusu 146×0 px).
 */
export const TOOLS_MIN_WORKSPACE_W = 560;

/**
 * Araçlar paneli varsayılan olarak açık gelmeli mi?
 *
 * `contentWidth`: kenar çubuğunun sağındaki içerik sütununun ölçülen
 * genişliği (pencere − kenar çubuğu). Panel açılıp kapandıkça DEĞİŞMEZ, bu
 * yüzden karar kendi sonucuna göre salınmaz. Ölçü yokken (sunucu tarafı
 * çizim) panel açık sayılır: araçlar hiçbir zaman sessizce saklanmaz.
 */
export function toolsPanelFits(contentWidth: number | null): boolean {
  if (contentWidth == null) return true;
  return contentWidth - INSPECTOR_SLOT_W >= TOOLS_MIN_WORKSPACE_W;
}

/** Kullanıcının bu kipte düğmeyle verdiği karar; kipten ayrılınca düşer. */
export interface ToolsChoice {
  route: string;
  open: boolean;
}

/** Kullanıcının seçimi yalnız verildiği kipte geçerlidir. */
export function activeToolsChoice(choice: ToolsChoice | null, route: string): boolean | null {
  return choice && choice.route === route ? choice.open : null;
}

/** Panel şu an çizilsin mi? */
export function inspectorShown({
  kind,
  detailsOpen,
  toolsChoice,
  contentWidth,
}: {
  /** Kip panel çizmiyorsa `null`. */
  kind: InspectorKind | null;
  /** Ayrıntılar panelinin (1280 kuralıyla yönetilen) durumu. */
  detailsOpen: boolean;
  /** Kullanıcının bu kipteki elle seçimi; yoksa `null`. */
  toolsChoice: boolean | null;
  contentWidth: number | null;
}): boolean {
  if (kind === null) return false;
  if (kind === "details") return detailsOpen;
  return toolsChoice ?? toolsPanelFits(contentWidth);
}

/** Bardaki panel düğmesinin adı: panelin ne olduğunu söyler. */
export function inspectorToggleLabel(kind: InspectorKind | null, open: boolean): string {
  if (kind === "tools") return open ? "Araçları Gizle" : "Araçları Göster";
  return open ? "Ayrıntıları Gizle" : "Ayrıntıları Göster";
}

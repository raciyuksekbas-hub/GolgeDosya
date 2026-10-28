/**
 * Ayır — sayfa aralığı ifadesinin çözümlenmesi ve çıktı planı.
 *
 * Bu dosya saf mantıktır: dosya yazmaz, motoru çağırmaz. Kullanıcının tek bir
 * satıra yazdığı ifadeyi ("1-3, 4-10, 11") parçalara çevirir ve ortaya ÇIKACAK
 * dosyaların listesini daha hiçbir şey yazılmadan önce verir. Araç bu listeyi
 * panelde gösterir: avukat "Ayır"a basmadan önce kaç dosya oluşacağını, her
 * birinde hangi sayfaların bulunacağını ve hangi sayfaların hiçbir dosyaya
 * girmediğini görür.
 *
 * Motor sözleşmesi değişmedi. Her parça, var olan `Seç` işlemiyle
 * (`ToolOperation::Select`) ayrı ayrı yayımlanır; bu dosya yalnız hangi
 * sayfaların hangi parçaya gideceğini ve parçanın adını söyler.
 */

export interface SplitPart {
    /** 1'den başlayan kaynak sayfa numarası. */
    start: number;
    end: number;
}

export type SplitPlan =
    | { ok: true; parts: SplitPart[] }
    | { ok: false; error: string };

/** İfadede kabul edilen ayırıcılar ve Word'den yapıştırılan uzun tireler. */
const SEPARATORS = /[,;\n]+/;
const DASHES = /[‐‑‒–—―]/g;

export const RANGE_EXAMPLE = 'Örnek: 1-3, 4-10, 11';
export const RANGE_PLACEHOLDER = '1-3, 4-10, 11';
export const RANGE_HINT = '“-3” baştan 3’e kadar, “8-” 8’den sona kadar.';

/**
 * İfadeyi parçalara çevirir.
 *
 * Kabul edilen biçimler: `7` (tek sayfa), `4-10` (aralık), `-3` (baştan 3'e),
 * `8-` (8'den sona). Aralıklar çakışabilir — aynı sayfayı iki parçaya koymak
 * geçerli bir istektir ve engellenmez; bu, bir ekin iki ayrı dilekçede
 * kullanıldığı gerçek bir iştir.
 *
 * Hata döndüğünde metin kullanıcıya olduğu gibi gösterilir: hangi parçanın
 * okunamadığını söyler, teknik terim taşımaz.
 */
export function parseRanges(expression: string, pageCount: number): SplitPlan {
    const cleaned = expression.replace(DASHES, '-').trim();
    if (!cleaned)
        return { ok: false, error: `Ayrılacak sayfa aralıklarını yazın. ${RANGE_EXAMPLE}` };

    const parts: SplitPart[] = [];
    for (const raw of cleaned.split(SEPARATORS)) {
        const token = raw.trim();
        if (!token) continue;

        const match = /^(\d*)\s*-\s*(\d*)$/.exec(token);
        const single = /^(\d+)$/.exec(token);
        if (!match && !single)
            return { ok: false, error: `“${token}” bir sayfa aralığı değil. ${RANGE_EXAMPLE}` };

        let start: number;
        let end: number;
        if (single) {
            start = end = Number(single[1]);
        } else {
            // "-" tek başına iki ucu da boş bırakır; bu bir aralık değildir.
            if (!match![1] && !match![2])
                return { ok: false, error: `“${token}” bir sayfa aralığı değil. ${RANGE_EXAMPLE}` };
            start = match![1] ? Number(match![1]) : 1;
            end = match![2] ? Number(match![2]) : pageCount;
        }

        if (start === 0 || end === 0)
            return { ok: false, error: 'Sayfa numaraları 1’den başlar.' };
        if (start > end)
            return { ok: false, error: `“${token}” tersten yazılmış; önce küçük sayfa gelir.` };
        if (end > pageCount)
            return { ok: false, error: `Belge ${pageCount} sayfa; ${end}. sayfa yok.` };

        parts.push({ start, end });
    }

    if (parts.length === 0)
        return { ok: false, error: `Ayrılacak sayfa aralıklarını yazın. ${RANGE_EXAMPLE}` };
    return { ok: true, parts };
}

/** Her sayfa kendi dosyasına. */
export function everyPage(pageCount: number): SplitPart[] {
    return Array.from({ length: Math.max(0, pageCount) }, (_, i) => ({ start: i + 1, end: i + 1 }));
}

/** Parçanın kaç sayfa taşıdığı. */
export function partPageCount(part: SplitPart): number {
    return part.end - part.start + 1;
}

/** Parçanın kapsadığı kaynak sayfa numaraları — motora bu liste gider. */
export function partPages(part: SplitPart): number[] {
    return Array.from({ length: partPageCount(part) }, (_, i) => part.start + i);
}

/**
 * Dosya adındaki sayfa eki: `01-03` ya da tek sayfada `07`.
 *
 * Numaralar belgenin basamak genişliğine göre sıfırla doldurulur, çünkü
 * `sayfa-2` ile `sayfa-10` birçok dosya yöneticisinde ve arşivde yanlış
 * sıralanır. Genişlik motorun Ek adlandırmasındaki mantığın aynısıdır.
 */
export function partLabel(part: SplitPart, pageCount: number): string {
    const width = String(Math.max(pageCount, 1)).length;
    const pad = (n: number) => String(n).padStart(width, '0');
    return part.start === part.end ? pad(part.start) : `${pad(part.start)}-${pad(part.end)}`;
}

/** `GolgeDosya-ayir-2026-10-28_221500-sayfa-01-03.pdf` */
export function partFileName(stem: string, part: SplitPart, pageCount: number): string {
    return `${stem}-sayfa-${partLabel(part, pageCount)}.pdf`;
}

/** Hiçbir parçaya girmeyen sayfalar. Unutulan aralık sessizce kaybolmasın. */
export function uncoveredPages(parts: readonly SplitPart[], pageCount: number): number[] {
    const covered = new Set<number>();
    for (const part of parts)
        for (const page of partPages(part)) covered.add(page);
    const left: number[] = [];
    for (let page = 1; page <= pageCount; page++)
        if (!covered.has(page)) left.push(page);
    return left;
}

/**
 * Sayfa listesini insanın okuduğu gibi toplar: `[1,2,3,7]` → `"1-3, 7"`.
 * Otuz sayfalık bir belgede unutulan sayfaları tek tek saymak okunmuyordu.
 */
export function describePages(pages: readonly number[]): string {
    const spans: string[] = [];
    let i = 0;
    while (i < pages.length) {
        let j = i;
        while (j + 1 < pages.length && pages[j + 1] === pages[j] + 1) j++;
        spans.push(i === j ? `${pages[i]}` : `${pages[i]}-${pages[j]}`);
        i = j + 1;
    }
    return spans.join(', ');
}

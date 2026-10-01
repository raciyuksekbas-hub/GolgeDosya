/**
 * Dosya boyutu ve küçülme oranı — kullanıcının okuyacağı TEK sayı dili.
 *
 * Sıkıştırma sonucu "13478.1 KB → 522.0 KB · %96.1" diye yazılıyordu: İngilizce
 * ondalık noktası, megabaytta bile KB ve kullanıcıya bir şey anlatmayan
 * ondalıklar. Aynı ekranda "kazanç yok" satırı ise "9,38 MB" diyordu; iki ayrı
 * sayı dili yan yanaydı.
 *
 * Kurallar:
 *   - 1 KB altı bayt; 1 MB altı tamsayı KB; üstü TEK ondalıklı MB, virgülle.
 *   - Taban 1024 (uygulamanın öteki boyutları ve Windows Gezgini gibi). macOS
 *     1000 tabanlı gösterir: 13.801.597 bayt orada 13,8 MB, burada 13,2 MB.
 *   - Yüzde tamsayıdır ve yaklaşıktır; sıfıra yuvarlanan kazanç "%1'den az".
 *   - Yuvarlama tamsayı aritmetiğiyle yapılır, `toFixed` kullanılmaz: o hem
 *     nokta koyar hem de tam yarım değerlerde beklenmedik yuvarlar.
 */

const KB = 1024;
const MB = 1024 * 1024;

/** `value / divisor`, yarım yukarı yuvarlanmış tamsayı. */
const roundedDiv = (value: number, divisor: number) => Math.floor((2 * value + divisor) / (2 * divisor));

/** "797 KB", "9,8 MB", "512 bayt". */
export function humanSize(bytes: number): string {
    const n = Math.max(0, Math.floor(bytes || 0));
    if (n < KB)
        return `${n} bayt`;
    const kb = roundedDiv(n, KB);
    // 1023,6 KB "1024 KB" değil, "1,0 MB" olarak okunur.
    if (kb < 1024)
        return `${kb} KB`;
    const tenths = roundedDiv(n * 10, MB);
    return `${Math.floor(tenths / 10)},${tenths % 10} MB`;
}

/**
 * Kaynaktan çıktıya küçülme: "%58", "%1'den az". Küçülme yoksa `null` —
 * büyüyen ya da aynı kalan bir dosya için oran uydurulmaz.
 */
export function approxReduction(sourceBytes: number, outputBytes: number): string | null {
    const source = Math.max(0, Math.floor(sourceBytes || 0));
    const output = Math.max(0, Math.floor(outputBytes || 0));
    if (source === 0 || output >= source)
        return null;
    const percent = roundedDiv((source - output) * 100, source);
    if (percent === 0)
        return '%1\'den az';
    // Bir dosya yok olmadıkça "%100 küçüldü" denmez.
    return `%${output > 0 ? Math.min(percent, 99) : 100}`;
}

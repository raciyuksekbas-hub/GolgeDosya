import { open, save } from '@tauri-apps/plugin-dialog';
import { join } from '@tauri-apps/api/path';

/**
 * Dosya adındaki damga: YEREL saat, `YYYY-MM-DD_HHMMSS`.
 *
 * Eskiden `toISOString()` kullanılıyordu, yani UTC: Türkiye'de ad üç saat
 * geri, 00:00–02:59 arasında bir gün önceki tarihi taşıyordu. Biçim
 * Dönüştür'ün `localStamp`'i ile aynı dildedir; saniye de vardır, çünkü
 * "Klasör Seçerek Kaydet" pencere açmadan yazar ve yayın üzerine yazmaz
 * (no-clobber): aynı dakikadaki ikinci kayıt çakışmamalı.
 */
export function fileStamp(now = new Date()): string {
    const p = (n: number) => String(n).padStart(2, '0');
    return `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}` +
        `_${p(now.getHours())}${p(now.getMinutes())}${p(now.getSeconds())}`;
}

/** `GolgeDosya-<araç>-<yerel damga>.pdf` */
export function copyName(base: string, now = new Date()): string {
    return `${base}-${fileStamp(now)}.pdf`;
}

/**
 * Both routes publish through the same backend no-clobber validation.
 *
 * `base` ürün adını taşır (`GolgeDosya-<araç>`): kaydedilen kopyanın adı
 * kullanıcıya görünür ve eski ürün adını taşımamalıdır. ASCII yazım bilerek:
 * dosya adı her dosya sisteminde ve her arşivde aynı kalsın.
 */
export async function copyDestination(base: string, folderOnly: boolean): Promise<string | null> {
    const name = copyName(base);
    if (!folderOnly) {
        const picked = await save({ defaultPath: name, filters: [{ name: 'Yeni PDF', extensions: ['pdf'] }] });
        return picked ? withPdfExtension(picked) : null;
    }
    const folder = await open({ directory: true, multiple: false, title: 'Yeni PDF kopyasının kaydedileceği klasör' });
    return typeof folder === 'string' ? join(folder, name) : null;
}

/**
 * Kaydetme penceresi uzantıyı garanti etmez: kullanıcı adı yazarken `.pdf`'yi
 * silebilir ve macOS bunu olduğu gibi döndürür. Motor dosyayı yazar, ama
 * kabuğun alım fişi (tarayıcı) biçimi UZANTIDAN okur ve uzantısız dosyayı
 * reddeder — kullanıcı dosya diskteyken "kaydedilemedi" görüyordu.
 * `.PDF` gibi büyük harfli yazım korunur; yalnız eksik uzantı tamamlanır.
 */
export function withPdfExtension(path: string): string {
    const trimmed = path.trimEnd();
    return /\.pdf$/i.test(trimmed) ? trimmed : `${trimmed}.pdf`;
}

import { open, save } from '@tauri-apps/plugin-dialog';
import { join } from '@tauri-apps/api/path';

/**
 * Both routes publish through the same backend no-clobber validation.
 *
 * `base` ürün adını taşır (`GolgeDosya-<araç>`): kaydedilen kopyanın adı
 * kullanıcıya görünür ve eski ürün adını taşımamalıdır. ASCII yazım bilerek:
 * dosya adı her dosya sisteminde ve her arşivde aynı kalsın.
 */
export async function copyDestination(base: string, folderOnly: boolean): Promise<string | null> {
    const name = `${base}-${new Date().toISOString().replace(/[:.]/g, '-')}.pdf`;
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

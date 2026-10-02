/**
 * Kaydedilen PDF'nin durum metni.
 *
 * Alındı (ad ve sayfa sayısı) yayımlanan dosyanın yeniden taranmasından
 * gelir. Dosyanın YERİ bu metinde değil, altındaki "Klasör" satırındadır
 * (`shared-ui/savePlace`): tam yol dar panelde satırlara sarılıyordu ve
 * öteki araçlar da yeri aynı satırla söylüyor. Sıkıştırmada
 * ilk satır sonucu insan diliyle söyler. Motorun iç ölçüsü ("Sıkıştırılmış
 * belge … · Marka dahil …") kullanıcı metnine girmez: aradaki ~1 KB'lık logo
 * payı avukata bir şey anlatmıyordu.
 */
import { approxReduction, humanSize } from '../../shared-ui/sizeText';
import { fileNameOf } from '../../shell/modes';

export interface SavedReceipt {
    kind: string;
    outputPath: string;
    pageCount: number;
    /** Yayımlanan dosyanın diskteki boyutu. */
    outputBytes: number;
    /** Kaynağın boyutu (motorun ölçtüğü, yoksa taranan). */
    sourceBytes: number;
}

/** "Sıkıştırıldı: 9,8 MB → 4,1 MB (yaklaşık %58 küçüldü)." */
export function compressResultLine(sourceBytes: number, outputBytes: number): string {
    const reduction = approxReduction(sourceBytes, outputBytes);
    const sizes = `${humanSize(sourceBytes)} → ${humanSize(outputBytes)}`;
    return reduction ? `Sıkıştırıldı: ${sizes} (yaklaşık ${reduction} küçüldü).` : `Sıkıştırıldı: ${sizes}.`;
}

export function savedStatus(receipt: SavedReceipt): string {
    const saved = `PDF kaydedildi ve yeniden açılarak doğrulandı: ${fileNameOf(receipt.outputPath)}`;
    if (receipt.kind === 'compress')
        return [compressResultLine(receipt.sourceBytes, receipt.outputBytes), saved, `${receipt.pageCount} sayfa`].join('\n');
    return [saved, `${receipt.pageCount} sayfa · ${humanSize(receipt.outputBytes)}`].join('\n');
}

/**
 * Sıkıştırma "kazanç yok" sonucu — kullanıcıya GERÇEK bayt sonucu.
 *
 * Eskiden "Bu belge zaten yeterince optimize." deniyordu. Motorun bulamadığı
 * küçülmeden belgenin optimum olduğu sonucu çıkarılamaz; sahada 204 KB'lık
 * gerçek bir kazanç bu cümlenin arkasında sessizce atılıyordu.
 *
 * Motor aynı cümleyi kurar (`crates/ekler-core/src/toolbox.rs` ·
 * `no_benefit_message`). Yuvarlama iki uçta da TAMSAYI aritmetiğiyle yapılır:
 * `toFixed` ile Rust'ın `{:.1}`'i tam yarım değerlerde farklı yuvarlar.
 * İki uç aynı örnek tablosuyla sınanır.
 */

const TAIL = 'küçültülebildi. Bu değer anlamlı küçülme eşiğinin altında kaldığı için çıktı oluşturulmadı.';

export const NO_MEANINGFUL_SAVING = 'Bu ayarlarla kayda değer bir küçülme sağlanamadı. Çıktı oluşturulmadı.';

/**
 * Yalnız kazanç YOKKEN gösterilir: markalı bir GölgeDosya çıktısı yine de çok
 * küçülebilir (sahada biri %38 küçüldü), bu yüzden sıkıştırmadan ÖNCE söylenmez.
 */
export const PREVIOUSLY_PROCESSED = 'Bu belge GölgeDosya tarafından daha önce işlenmiş. Ek küçülme sınırlı olabilir.';

/** `value / divisor`, yarım yukarı yuvarlanmış tamsayı. */
const roundedDiv = (value: number, divisor: number) => Math.floor((2 * value + divisor) / (2 * divisor));

/** Onda birlik tamsayıyı "2,1" biçimine çevirir. */
const tenths = (n: number) => `${Math.floor(n / 10)},${n % 10}`;

export function noBenefitMessage(sourceBytes: number, candidateBytes: number): string {
    const saved = Math.max(0, sourceBytes - candidateBytes);
    const percentTenths = roundedDiv(saved * 1000, Math.max(1, sourceBytes));
    // Yuvarlanınca %0,0 çıkan kazanç için "0 KB küçültülebildi" gibi bir sayı uydurulmaz.
    if (saved === 0 || percentTenths === 0)
        return NO_MEANINGFUL_SAVING;
    const size = saved < 1024
        ? `${saved} bayt`
        : saved < 10 * 1024
            ? `${tenths(roundedDiv(saved * 10, 1024))} KB`
            : `${roundedDiv(saved, 1024)} KB`;
    return `${size} (%${tenths(percentTenths)}) ${TAIL}`;
}

/** Alt bilgi boyutu: 1 MB'ın altında KB, üstünde iki ondalıklı MB. */
export function sizeLabel(bytes: number): string {
    const MB = 1024 * 1024;
    if (bytes < MB)
        return `${roundedDiv(bytes, 1024)} KB`;
    const hundredths = roundedDiv(bytes * 100, MB);
    return `${Math.floor(hundredths / 100)},${String(hundredths % 100).padStart(2, '0')} MB`;
}

export interface NoBenefitOutcome {
    source_bytes?: number;
    candidate_bytes?: number;
    previously_processed?: boolean;
}

/** Durum alanına yazılacak bütün metin: sonuç · alt bilgi · (varsa) bağlam. */
export function noBenefitStatus(outcome: NoBenefitOutcome): string {
    const source = outcome.source_bytes ?? 0;
    const candidate = outcome.candidate_bytes ?? 0;
    const lines = [
        noBenefitMessage(source, candidate),
        `Kaynak: ${sizeLabel(source)} · Olası çıktı: ${sizeLabel(candidate)}`,
    ];
    if (outcome.previously_processed)
        lines.push(PREVIOUSLY_PROCESSED);
    return lines.join('\n');
}

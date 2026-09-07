/**
 * React 18 ↔ 19 ref tipi köprüsü.
 *
 * React 19, `RefObject<T>.current`'ı non-null yaptı ve boş olabilen hâl için
 * `RefObject<T | null>` biçimini getirdi. Bağımsız Değişikİş React 19'da
 * yazıldığı için ref tiplerini o sözleşmeye göre bildiriyor; birleşik kabuk
 * React 18 hattında.
 *
 * Fark YALNIZ tip katmanındadır: `useRef` çalışma zamanında iki sürümde de aynı
 * şeyi yapar. Bu yüzden burada React'in kendi tipleri yerine iki sürümde de
 * yapısal olarak eşleşen iki takma ad kullanılıyor. Böylece ne kabuk React 19'a
 * yükseltilmek zorunda kalıyor ne de karşılaştırma motorunun bir satırı
 * değişiyor — değişen tek şey iki UI dosyasının tip bildirimi.
 *
 * Kabuk React 19'a taşındığında bu dosya silinebilir; ondan önce silinemez.
 */

/** Değeri her zaman var olan ref (React 19 `RefObject<T>`, React 18 `MutableRefObject<T>`). */
export type FilledRef<T> = { readonly current: T };

/** Değeri boş olabilen ref (React 19 `RefObject<T | null>`, React 18 `RefObject<T>`). */
export type NullableRef<T> = { readonly current: T | null };

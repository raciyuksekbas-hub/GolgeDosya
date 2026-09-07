export const DOCUMENT_FILE_ACCEPT = ".pdf,.doc,.docx,.udf";
export const DOCUMENT_FILE_TYPES_LABEL = "PDF · DOC · DOCX · UDF";

export type DocumentSide = "base" | "revised";

const SUPPORTED_EXTENSIONS = ["pdf", "doc", "docx", "udf"];
/** İki belge karşılaştırıldığı için tek seferde en fazla iki dosya alınır. */
export const MAX_SELECTED_FILES = 2;

export function firstDroppedFile(transfer: Pick<DataTransfer, "files">): File | undefined {
  return transfer.files[0];
}

export function droppedFiles(transfer: Pick<DataTransfer, "files">): File[] {
  return Array.from(transfer.files);
}

export function fileExtension(name: string): string {
  const match = /\.([^.]+)$/u.exec(name);
  return match ? match[1].toLocaleLowerCase("en-US") : "";
}

export function isSupportedDocument(file: Pick<File, "name">): boolean {
  return SUPPORTED_EXTENSIONS.includes(fileExtension(file.name));
}

export interface DropPlan {
  assignments: Array<{ side: DocumentSide; file: File }>;
  error?: string;
}

export interface DropContext {
  baseFilled: boolean;
  revisedFilled: boolean;
  /** Belge belirli bir panelin üzerine bırakıldıysa o taraf. */
  target?: DocumentSide;
}

/**
 * Bırakılan dosyaların hangi sürüm yuvasına gideceğini belirler. Kural dışı
 * durumlarda sessizce dosya düşürmek yerine açık bir hata döndürür; çağıran
 * bunu kullanıcıya gösterir.
 */
export function planFileDrop(files: File[], context: DropContext): DropPlan {
  if (!files.length) return { assignments: [], error: "Bırakılan öğede dosya bulunamadı." };

  const unsupported = files.filter((file) => !isSupportedDocument(file));
  if (unsupported.length) {
    const names = unsupported.map((file) => file.name).join(", ");
    return {
      assignments: [],
      error: `Desteklenmeyen dosya: ${names}. ${DOCUMENT_FILE_TYPES_LABEL} biçimleri kabul edilir.`,
    };
  }

  if (files.length > MAX_SELECTED_FILES) {
    return {
      assignments: [],
      error: `Aynı anda en fazla ${MAX_SELECTED_FILES} belge bırakabilirsiniz. ${files.length} dosya bırakıldı.`,
    };
  }

  if (files.length === MAX_SELECTED_FILES) {
    return { assignments: [{ side: "base", file: files[0] }, { side: "revised", file: files[1] }] };
  }

  const [file] = files;
  if (context.target) return { assignments: [{ side: context.target, file }] };
  if (!context.baseFilled) return { assignments: [{ side: "base", file }] };
  if (!context.revisedFilled) return { assignments: [{ side: "revised", file }] };
  return {
    assignments: [],
    error: "İki sürüm de dolu. Değiştirmek istediğiniz sürümün paneline bırakın.",
  };
}

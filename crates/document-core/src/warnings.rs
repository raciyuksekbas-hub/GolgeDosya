//! Structured fidelity warnings.
//!
//! Rule: a feature that could not be carried across is ALWAYS reported. Silence means
//! "preserved". No code path may drop a meaningful document feature without pushing a
//! warning here.

use serde::{Deserialize, Serialize};

/// How much a transformation actually cost the document.
///
/// The distinction that matters: an *expected* transformation with no visible consequence
/// (an electronic signature that was deliberately not copied) must not colour the result
/// the same way as a *visible* loss (a label that disappeared). Overstating harmless
/// behaviour trains the user to ignore the panel, which is how a real loss gets missed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Severity {
    /// Expected, deliberate, no visible content or formatting lost.
    Info,
    /// Usable, but a formatting detail could only be approximated.
    Approximation,
    /// A visible property of the document was actually lost.
    Loss,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "INFO",
            Severity::Approximation => "APPROXIMATION",
            Severity::Loss => "LOSS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WarningCode {
    // --- run level ---
    RunFeatureDropped,
    // --- paragraph level ---
    TabAlignmentApproximated,
    TabStopsTrimmed,
    LineBreakConverted,
    ListNumberingApproximated,
    ListNestingUnverified,
    // --- table level ---
    TableVerticalMergeFlattened,
    // Artık üretilmiyor: iç içe tablo UDF'e olduğu gibi yazılır (udf/writer.rs).
    // Sonuç sözleşmesindeki kod adı değişmesin diye tutulur.
    TableNestedFlattened,
    TableBordersApproximated,
    TableColumnWidthsApproximated,
    // --- images ---
    ImageTranscodedToPng,
    ImageFormatUnsupported,
    // --- page / section ---
    MultiSectionFlattened,
    PageOrientationUnverified,
    PageNumberFieldDropped,
    PageBreakDropped,
    // --- document level ---
    SignatureDropped,
    UdfFormFieldsDropped,
    TrackedChangesFlattened,
    CommentsDropped,
    FootnotesDropped,
    HyperlinkFlattened,
    TextboxFlattened,
    ShapeDropped,
    EquationDropped,
    ContentControlFlattened,
    FieldFlattened,
    EmbeddedObjectDropped,
    HeaderFooterVariantDropped,
    UnknownFormatVersion,
    FormFieldsStaticized,
    FormContentLost,
    PageSizeChanged,
    PageNumberFieldApproximated,
    PageNumberStartDropped,
    // --- bozuk UDF konumları ---
    TextRangeOverlap,
    TextRangeInvalid,
    UdfPageNumberApproximated,
    UnrecognizedTextDropped,
}

impl WarningCode {
    pub fn as_str(self) -> &'static str {
        use WarningCode::*;
        match self {
            RunFeatureDropped => "RUN_FEATURE_DROPPED",
            TabAlignmentApproximated => "TAB_ALIGNMENT_APPROXIMATED",
            TabStopsTrimmed => "TAB_STOPS_TRIMMED",
            LineBreakConverted => "LINE_BREAK_CONVERTED",
            ListNumberingApproximated => "LIST_NUMBERING_APPROXIMATED",
            ListNestingUnverified => "LIST_NESTING_UNVERIFIED",
            TableVerticalMergeFlattened => "TABLE_VERTICAL_MERGE_FLATTENED",
            TableNestedFlattened => "TABLE_NESTED_FLATTENED",
            TableBordersApproximated => "TABLE_BORDERS_APPROXIMATED",
            TableColumnWidthsApproximated => "TABLE_COLUMN_WIDTHS_APPROXIMATED",
            ImageTranscodedToPng => "IMAGE_TRANSCODED_TO_PNG",
            ImageFormatUnsupported => "IMAGE_FORMAT_UNSUPPORTED",
            MultiSectionFlattened => "MULTI_SECTION_FLATTENED",
            PageOrientationUnverified => "PAGE_ORIENTATION_UNVERIFIED",
            PageNumberFieldDropped => "PAGE_NUMBER_FIELD_DROPPED",
            PageBreakDropped => "PAGE_BREAK_DROPPED",
            SignatureDropped => "SIGNATURE_DROPPED",
            UdfFormFieldsDropped => "UDF_FORM_FIELDS_DROPPED",
            TrackedChangesFlattened => "TRACKED_CHANGES_FLATTENED",
            CommentsDropped => "COMMENTS_DROPPED",
            FootnotesDropped => "FOOTNOTES_DROPPED",
            HyperlinkFlattened => "HYPERLINK_FLATTENED",
            TextboxFlattened => "TEXTBOX_FLATTENED",
            ShapeDropped => "SHAPE_DROPPED",
            EquationDropped => "EQUATION_DROPPED",
            ContentControlFlattened => "CONTENT_CONTROL_FLATTENED",
            FieldFlattened => "FIELD_FLATTENED",
            EmbeddedObjectDropped => "EMBEDDED_OBJECT_DROPPED",
            HeaderFooterVariantDropped => "HEADER_FOOTER_VARIANT_DROPPED",
            UnknownFormatVersion => "UNKNOWN_FORMAT_VERSION",
            FormFieldsStaticized => "FORM_FIELDS_STATICIZED",
            FormContentLost => "FORM_CONTENT_LOST",
            PageSizeChanged => "PAGE_SIZE_CHANGED",
            PageNumberFieldApproximated => "PAGE_NUMBER_FIELD_APPROXIMATED",
            PageNumberStartDropped => "PAGE_NUMBER_START_DROPPED",
            TextRangeOverlap => "TEXT_RANGE_OVERLAP",
            TextRangeInvalid => "TEXT_RANGE_INVALID",
            UdfPageNumberApproximated => "UDF_PAGE_NUMBER_APPROXIMATED",
            UnrecognizedTextDropped => "UNRECOGNIZED_TEXT_DROPPED",
        }
    }

    /// Turkish sentence template shown in the "kısmen korundu" list.
    pub fn title_tr(self) -> &'static str {
        use WarningCode::*;
        match self {
            RunFeatureDropped => "Bazı karakter biçimleri aktarılamadı",
            TabAlignmentApproximated => "Sekme hizalaması yaklaşık aktarıldı",
            TabStopsTrimmed => "Sayfa genişliğini aşan sekme durakları çıkarıldı",
            LineBreakConverted => "Paragraf içi satır sonları boşluğa çevrildi",
            ListNumberingApproximated => "Liste numaralandırma biçimi yaklaşık aktarıldı",
            ListNestingUnverified => {
                "İç içe liste düzeyleri aktarıldı, görünümü UYAP'ta doğrulanmalı"
            }
            TableVerticalMergeFlattened => "Dikey birleştirilmiş tablo hücreleri düzleştirildi",
            TableNestedFlattened => "İç içe tablo düzleştirildi",
            // UDF bir tabloda yalnız "bütün hücre kenarlıkları" ya da "hiç kenarlık
            // yok" tutabiliyor. Kısmen kenarlıklı Word tablosu bütün kenarlıklarıyla
            // çizilir; başlık sonucu söyler.
            TableBordersApproximated => {
                "Tablonun yalnız bazı kenarlıkları vardı; UDF'de bütün hücre kenarlıkları çizildi"
            }
            // Birleşik UDF satırı kendi hücre genişliklerini taşır; Word'de hücre
            // tablonun sütun ızgarasına oturur. Satırın bir sınırı hiçbir sütun
            // sınırına yakın değilse en yakın sınır kullanılır ve hücre genişliği
            // değişir; başlık sonucu söyler.
            TableColumnWidthsApproximated => {
                "Birleştirilmiş tablo satırlarında bazı hücre genişlikleri yaklaşık aktarıldı"
            }
            ImageTranscodedToPng => "Görsel PNG biçimine dönüştürüldü",
            ImageFormatUnsupported => "Görsel biçimi desteklenmiyor ve aktarılamadı",
            MultiSectionFlattened => "Çoklu bölüm düzeni tek bölüme indirildi",
            PageOrientationUnverified => "Yatay sayfa yönü aktarıldı, UYAP'ta doğrulanmalı",
            PageNumberFieldDropped => "Sayfa numarası alanı aktarılamadı",
            // Metin eksiksiz aktarılır; yalnız sonraki bölüm yeni sayfadan
            // başlamaz. 479 gerçek UDF'te sayfa sonunu taşıyan öğe yok.
            PageBreakDropped => "Sayfa sonları UDF'te karşılıksız; metin aynı sayfadan sürer",
            SignatureDropped => "Belgedeki elektronik imza aktarılamadı",
            UdfFormFieldsDropped => "UYAP form alanları düz metne çevrildi",
            TrackedChangesFlattened => "Değişiklik izleme kayıtları düzleştirildi",
            CommentsDropped => "Belgedeki açıklamalar aktarılamadı",
            FootnotesDropped => "Dipnot ve son notlar aktarılamadı",
            HyperlinkFlattened => "Köprüler düz metne çevrildi",
            TextboxFlattened => "Metin kutuları düz paragrafa çevrildi",
            ShapeDropped => "Çizim nesneleri aktarılamadı",
            EquationDropped => "Denklemler aktarılamadı",
            ContentControlFlattened => "İçerik denetimleri düz metne çevrildi",
            FieldFlattened => "Alan kodları son görünen değerine sabitlendi",
            EmbeddedObjectDropped => "Gömülü nesneler aktarılamadı",
            HeaderFooterVariantDropped => "Bazı üst/alt bilgi çeşitleri aktarılamadı",
            UnknownFormatVersion => "Belgenin biçim sürümü tanınmıyor",
            FormFieldsStaticized => {
                "UYAP'a özgü form alanları düzenlenebilir Word metnine dönüştürüldü"
            }
            FormContentLost => "Bazı form alanlarının görünür içeriği aktarılamadı",
            PageSizeChanged => "Sayfa boyutu değiştirildi",
            // Yazıcı hizayı bilmez ve numarayı ORTALAR, mevcut alt/üst bilgi
            // metninin altına AYRI bir paragraf olarak ekler (docx/writer.rs,
            // page_number_paragraph). UDF'nin hizayı nasıl kodladığı örnekle
            // kanıtlanmadıkça tahmin edilmez; ama başlık bunu saklamaz.
            // Başlık sonucu söyler, mekanizmayı değil: "yeniden oluşturuldu"
            // sahada "bozuldu da onarıldı" diye okundu. "Korundu" da denmez,
            // çünkü konum korunmuyor.
            PageNumberFieldApproximated => {
                "Sayfa numarası Word'ün otomatik sayfa numarasına çevrildi (ortalı, ayrı satırda)"
            }
            // pageNumber-pageStartNumStr okunur ama Word'e yazılmaz: anlamı gerçek
            // bir UYAP örneğiyle doğrulanmadan <w:pgNumType w:start> tahmin
            // edilmez. Arayüz yalnız başlığı gösterir, bu yüzden sonuç başlıkta.
            PageNumberStartDropped => {
                "Sayfa numarası 1'den başlıyor; kaynaktaki başlangıç numarası aktarılmadı"
            }
            // UDF'in metin öğeleri tek bir tampona (startOffset, length) ile
            // başvurur; gerçek UYAP belgesi her birime bir kez başvurur (293
            // belgede çakışma 0). Çakışan kısım iki kez yazılmaz; başlık bunu
            // söyler ki metin kontrol edilsin.
            TextRangeOverlap => {
                "Belgede üst üste binen metin aralıkları vardı; tekrar eden metin bir kez aktarıldı"
            }
            // Konumu okunamayan ya da tamponun dışına düşen öğe: metni
            // bilinemez. Eskiden konum 0 sayılıp ilk satır yineleniyordu.
            TextRangeInvalid => "Konumu bozuk olduğu için bazı metin parçaları aktarılamadı",
            // Word → UDF yönü. `PageNumberFieldApproximated`'ın başlığı Word
            // çıktısını anlatır ("ortalı, ayrı satırda"); bu yönde doğru olmaz.
            // Numara canlı kalır, ama UYAP'ın yerleşimi (`pageNumber-spec`)
            // korpusta en sık görülen değerden seçilir: Word'deki yeri ve hizası
            // korunmuş sayılmaz.
            UdfPageNumberApproximated => {
                "Sayfa numarası UYAP'ın otomatik sayfa numarasına çevrildi; yeri ve görünümü yaklaşık"
            }
            // UDF okuyucusunun güvenlik ağı: tamponda görünen metnin bir kısmı
            // tanınmayan bir yapıda durduğu için modele ulaşmadı. Ayrıntı kaç
            // karakter olduğunu söyler; metnin kendisi uyarıya girmez.
            UnrecognizedTextDropped => {
                "Belgedeki bazı metinler tanınmayan bir yapıda olduğu için aktarılamadı"
            }
        }
    }

    /// What this warning actually cost the document.
    pub fn severity(self) -> Severity {
        use Severity::*;
        use WarningCode::*;
        match self {
            // Expected transformations. Nothing a reader of the document would miss.
            SignatureDropped
            | FormFieldsStaticized
            | LineBreakConverted
            | UnknownFormatVersion
            | PageNumberFieldApproximated
            | UdfPageNumberApproximated => Info,

            // Usable, but a formatting detail is only approximate.
            TabAlignmentApproximated
            | TabStopsTrimmed
            | ListNumberingApproximated
            | ListNestingUnverified
            | TableNestedFlattened
            | TableBordersApproximated
            | TableColumnWidthsApproximated
            | ImageTranscodedToPng
            | PageOrientationUnverified
            | MultiSectionFlattened
            | HyperlinkFlattened
            | TextboxFlattened
            | ContentControlFlattened
            | FieldFlattened
            | TrackedChangesFlattened
            | RunFeatureDropped
            | TableVerticalMergeFlattened
            | PageNumberStartDropped
            | TextRangeOverlap
            // Metin eksiksizdir; sayfa düzeni değişir. "İçerik kaybı" değildir:
            // başlık öyle deyince kullanıcı dönüştürmeyi hata sandı (saha).
            | PageBreakDropped => Approximation,

            // Something visible is gone.
            FormContentLost
            | PageNumberFieldDropped
            | ImageFormatUnsupported
            | CommentsDropped
            | FootnotesDropped
            | ShapeDropped
            | EquationDropped
            | EmbeddedObjectDropped
            | HeaderFooterVariantDropped
            | UdfFormFieldsDropped
            | PageSizeChanged
            | TextRangeInvalid
            | UnrecognizedTextDropped => Loss,
        }
    }

    /// Retained for the result contract: true only for genuine loss.
    pub fn is_data_loss(self) -> bool {
        self.severity() == Severity::Loss
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub code: WarningCode,
    pub severity: Severity,
    /// Human-readable Turkish title.
    pub title: String,
    /// Where in the document, e.g. "Tablo 2" or "Paragraf 41". Never document text.
    pub location: Option<String>,
    /// What specifically was affected, e.g. "üstü çizili", "EMF".
    pub detail: Option<String>,
    pub data_loss: bool,
}

impl Warning {
    pub fn new(code: WarningCode) -> Self {
        Self {
            code,
            severity: code.severity(),
            title: code.title_tr().to_string(),
            location: None,
            detail: None,
            data_loss: code.is_data_loss(),
        }
    }

    pub fn at(mut self, loc: impl Into<String>) -> Self {
        self.location = Some(loc.into());
        self
    }

    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
}

/// Accumulates warnings, collapsing repeats of the same (code, detail) pair into one entry
/// with a count so a 300-page document does not produce 300 identical lines.
#[derive(Debug, Default, Clone)]
pub struct WarningSink {
    items: Vec<Warning>,
    counts: Vec<usize>,
}

impl WarningSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, w: Warning) {
        if let Some(i) = self
            .items
            .iter()
            .position(|e| e.code == w.code && e.detail == w.detail)
        {
            self.counts[i] += 1;
            // Keep the first location; replace with a count once it stops being one place.
            if self.counts[i] == 2 {
                self.items[i].location = None;
            }
        } else {
            self.items.push(w);
            self.counts.push(1);
        }
    }

    pub fn warn(&mut self, code: WarningCode) {
        self.push(Warning::new(code));
    }

    /// Ayrı toplanmış uyarıları, tekrar sayılarıyla birlikte buraya aktarır.
    ///
    /// Okuyucu bir parçayı (ör. Word'ün ilk sayfa üst bilgisi) kullanıp
    /// kullanmayacağına okuduktan sonra karar verir; kullanılmayan parçanın
    /// uyarıları belgeye ait değildir.
    pub fn absorb(&mut self, other: WarningSink) {
        for (w, n) in other.items.into_iter().zip(other.counts) {
            for _ in 0..n {
                self.push(w.clone());
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn has(&self, code: WarningCode) -> bool {
        self.items.iter().any(|w| w.code == code)
    }

    /// The highest severity present, or `None` when nothing was reported.
    pub fn max_severity(&self) -> Option<Severity> {
        self.items.iter().map(|w| w.severity).max()
    }

    /// Finalise, annotating collapsed entries with how many times they occurred.
    pub fn into_vec(self) -> Vec<Warning> {
        let mut out: Vec<Warning> = self
            .items
            .into_iter()
            .zip(self.counts)
            .map(|(mut w, n)| {
                if n > 1 {
                    w.location = Some(format!("{} yerde", n));
                }
                w
            })
            .collect();
        // Data loss first, then alphabetical by code for deterministic output.
        out.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| a.code.as_str().cmp(b.code.as_str()))
                .then_with(|| a.detail.cmp(&b.detail))
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_collapse_with_a_count() {
        let mut s = WarningSink::new();
        for i in 0..5 {
            s.push(Warning::new(WarningCode::TableVerticalMergeFlattened).at(format!("Tablo {i}")));
        }
        let v = s.into_vec();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].location.as_deref(), Some("5 yerde"));
    }

    #[test]
    fn distinct_details_stay_separate() {
        let mut s = WarningSink::new();
        s.push(Warning::new(WarningCode::RunFeatureDropped).detail("üstü çizili"));
        s.push(Warning::new(WarningCode::RunFeatureDropped).detail("üst simge"));
        assert_eq!(s.into_vec().len(), 2);
    }

    #[test]
    fn severity_sorts_first_and_output_is_deterministic() {
        let mut s = WarningSink::new();
        s.warn(WarningCode::SignatureDropped); // INFO
        s.warn(WarningCode::TabAlignmentApproximated); // APPROXIMATION
        s.warn(WarningCode::FootnotesDropped); // LOSS
        let v = s.into_vec();
        assert_eq!(
            v[0].code,
            WarningCode::FootnotesDropped,
            "real loss must lead"
        );
        assert_eq!(v[0].severity, Severity::Loss);
        assert_eq!(
            v[2].code,
            WarningCode::SignatureDropped,
            "info must come last"
        );
        assert_eq!(v[2].severity, Severity::Info);
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn a_deliberately_omitted_signature_is_not_a_fidelity_loss() {
        // Not copying the e-signature is product policy, not damage. Treating it as a
        // warning trains the user to ignore the panel that reports real losses.
        assert_eq!(WarningCode::SignatureDropped.severity(), Severity::Info);
        assert!(!WarningCode::SignatureDropped.is_data_loss());
        // Staticizing a form is also expected, as long as the text survived.
        assert_eq!(WarningCode::FormFieldsStaticized.severity(), Severity::Info);
        // Losing visible form text is not.
        assert_eq!(WarningCode::FormContentLost.severity(), Severity::Loss);
    }

    #[test]
    fn max_severity_reports_the_worst_present() {
        let mut s = WarningSink::new();
        assert_eq!(s.max_severity(), None);
        s.warn(WarningCode::SignatureDropped);
        assert_eq!(s.max_severity(), Some(Severity::Info));
        s.warn(WarningCode::TabAlignmentApproximated);
        assert_eq!(s.max_severity(), Some(Severity::Approximation));
        s.warn(WarningCode::FootnotesDropped);
        assert_eq!(s.max_severity(), Some(Severity::Loss));
    }

    #[test]
    fn single_occurrence_keeps_its_location() {
        let mut s = WarningSink::new();
        s.push(Warning::new(WarningCode::TableNestedFlattened).at("Tablo 3"));
        assert_eq!(s.into_vec()[0].location.as_deref(), Some("Tablo 3"));
    }
}

#[cfg(test)]
mod page_break_tests {
    use super::*;

    /// Saha: bir dilekçede tek bir "önce sayfa sonu" ayarı başlığı
    /// "Dönüştürme tamamlandı — içerik kaybı var" yapıyordu; kullanıcı bunu
    /// hata sandı. Metin eksiksiz aktarılır, yalnız yerleşim değişir.
    #[test]
    fn a_dropped_page_break_is_a_layout_change_not_content_loss() {
        assert_eq!(
            WarningCode::PageBreakDropped.severity(),
            Severity::Approximation
        );
        assert!(!WarningCode::PageBreakDropped.is_data_loss());
    }
}

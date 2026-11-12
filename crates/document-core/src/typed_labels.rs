//! Elle yazılmış madde numarasından sonraki boşluklar (Word → UDF).
//!
//! Saha: "4. ve 5. yazdıktan sonra birkaç boşluk oluyor." Word'de numara çoğu
//! zaman liste değil, elle yazılmıştır: asılı girintili paragrafta `4.␠␠Metin`.
//! Paragraf iki yana yaslıysa UYAP bu boşlukları satırdaki diğer boşluklar gibi
//! gerer; metin her maddede başka yerden başlar ve alt satırların hizasına
//! ulaşmaz. Boşluklar tek sekmeye çevrilir, durak asılı girinti noktasına konur:
//! dilekçe başlığındaki biçimin aynısı (`layout_tables`, `tests/corpus.rs` c50).
//! Kullanıcı aynı düzeltmeyi UYAP'ta elle yaptığında UYAP da maddeyi böyle
//! kaydetti: numara, sekme, `Hanging` ve `TabSet`.
//!
//! Numaradan sonra zaten sekme varsa (`1.⇥Metin`) yalnız durak eklenir. Word
//! asılı girintili paragrafta ilk sekmeyi girinti noktasına götürür (örtük
//! durak); UDF'te bu durak yazılmayınca UYAP sekmeyi varsayılan durağa (72 pt)
//! atıyordu ve metin alt satırlardan çok ileride başlıyordu (saha, istinaf
//! dilekçesi).
//!
//! Bilerek dar tutuldu; biçimi tutmayan paragraf yazıldığı gibi kalır:
//! - En az iki boşluk gerekir. Tek boşluk ayraç olmayabilir: "2. Hukuk Dairesi",
//!   "A. Yılmaz" birer cümle başıdır.
//! - İlk satır kenar boşluğunda olmalıdır. Girintili ilk satırda durağın neye
//!   göre ölçüldüğü UYAP'ta doğrulanmadı.
//! - Numara asılı girintiye sığmalıdır; sığmazsa sekme bir sonraki durağa atlar.
//! - Yalnız gövde paragrafları: tablo hücresindeki sekme durakları doğrulanmadı.

use crate::font_metrics::text_width;
use crate::layout_tables::TAB_CLEARANCE_PT;
use crate::model::*;

/// Bu kadar boşluk, numaranın ayracı olduğunu gösterir.
const MIN_SPACES: usize = 2;
const SAME_POS_PT: f32 = 0.01;

/// Gövde paragraflarındaki elle yazılmış numaraları asılı girintiye hizalar.
pub(crate) fn align_typed_labels(blocks: &mut [Block]) {
    for block in blocks {
        if let Block::Paragraph(p) = block {
            align(p);
        }
    }
}

fn align(p: &mut Paragraph) {
    let left = p.props.left_indent_pt;
    let shape = p.props.hanging_pt > 0.0
        && (left - p.props.hanging_pt).abs() < SAME_POS_PT
        && p.props.list.is_none()
        && matches!(p.props.alignment, Alignment::Left | Alignment::Justify)
        && !p
            .props
            .tab_stops
            .iter()
            .any(|t| t.pos_pt < left - SAME_POS_PT);
    if !shape {
        return;
    }
    if let Some((gap_start, gap_end)) = label_gap(p, left) {
        replace_with_tab(p, gap_start, gap_end);
    } else if !p.runs.iter().any(|r| matches!(r, Run::Tab { .. })) {
        return;
    }
    if !p
        .props
        .tab_stops
        .iter()
        .any(|t| (t.pos_pt - left).abs() < SAME_POS_PT)
    {
        p.props.tab_stops.insert(
            0,
            TabStop {
                pos_pt: left,
                align: TabAlign::Left,
            },
        );
    }
}

/// Numaradan sonraki boşlukların bayt aralığı (baştaki metin parçaları birlikte
/// sayılır: Word numarayı ve boşlukları ayrı parçalara bölebilir).
fn label_gap(p: &Paragraph, stop_pt: f32) -> Option<(usize, usize)> {
    let head: String = p
        .runs
        .iter()
        .map_while(|r| match r {
            Run::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let label_end = head.find(char::is_whitespace)?;
    let label = &head[..label_end];
    if !is_label(label) {
        return None;
    }
    let rest = &head[label_end..];
    let body = rest.trim_start_matches(' ');
    let spaces = rest.len() - body.len();
    if spaces < MIN_SPACES || body.is_empty() || body.starts_with(char::is_whitespace) {
        return None;
    }
    let props = p.runs.first()?.props()?;
    (text_width(label, props) <= stop_pt - TAB_CLEARANCE_PT)
        .then_some((label_end, label_end + spaces))
}

/// `4.`, `11.`, `1.2.`, `A.`, `a)`, `(1)`, `1-`, `IV.` gibi bir madde numarası mı?
fn is_label(token: &str) -> bool {
    let core = match token.strip_prefix('(') {
        Some(rest) => rest.strip_suffix(')'),
        None => token.strip_suffix(['.', ')', '-', '–']),
    };
    core.is_some_and(|c| is_number(c) || is_letter(c) || is_roman(c))
}

/// `4`, `1.2`; dört haneli yıl numara sayılmaz.
fn is_number(core: &str) -> bool {
    core.split('.')
        .all(|n| (1..=3).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()))
}

fn is_letter(core: &str) -> bool {
    let mut chars = core.chars();
    chars.next().is_some_and(char::is_alphabetic) && chars.next().is_none()
}

/// Madde numarası olarak görülen aralık (L'ye kadar). `md.`, `Cd.` gibi
/// kısaltmalar bu yüzden numara sayılmaz.
fn is_roman(core: &str) -> bool {
    (1..=7).contains(&core.len())
        && (core.bytes().all(|b| b"IVXL".contains(&b))
            || core.bytes().all(|b| b"ivxl".contains(&b)))
}

/// Baştaki metin parçalarında `start..end` aralığını tek sekmeyle değiştirir.
fn replace_with_tab(p: &mut Paragraph, start: usize, end: usize) {
    let mut runs = Vec::with_capacity(p.runs.len() + 2);
    let mut pos = 0;
    let mut tab_placed = false;
    let mut in_head = true;
    for run in std::mem::take(&mut p.runs) {
        let Run::Text { text, props } = run else {
            in_head = false;
            runs.push(run);
            continue;
        };
        let (from, to) = (pos, pos + text.len());
        pos = to;
        if !in_head || to <= start || from >= end {
            runs.push(Run::Text { text, props });
            continue;
        }
        let before = &text[..start.saturating_sub(from)];
        let after = &text[end.min(to) - from..];
        if !before.is_empty() {
            runs.push(Run::Text {
                text: before.to_string(),
                props: props.clone(),
            });
        }
        if !tab_placed {
            runs.push(Run::Tab {
                props: props.clone(),
            });
            tab_placed = true;
        }
        if !after.is_empty() {
            runs.push(Run::Text {
                text: after.to_string(),
                props,
            });
        }
    }
    p.runs = runs;
}

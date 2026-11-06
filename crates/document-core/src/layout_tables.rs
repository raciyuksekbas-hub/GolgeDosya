//! Dilekçe başlığındaki görünmez tabloyu sekmeli satırlara çevirme (Word → UDF).
//!
//! Saha: "Dilekçeler Word'den UDF'ye çevirirken blok hâlinde oluyor." Word
//! şablonlarında Dosya No. / Davacı / Vekili / Konu bloğu çoğu zaman kenarlıksız
//! bir tablodur; Word'de tablo olduğu görünmez. UDF'e tablo olarak yazılınca
//! UYAP onu blok gibi seçtiriyor, metin hücre hücre düzenleniyor. UYAP'ta
//! yazılan dilekçelerin biçimi sekmeli satırdır: `Etiket⇥:⇥Değer`, değer
//! sütununda asılı girinti ve kenar boşluğundan ölçülen sekme durakları
//! (`tests/corpus.rs` c50, UYAP kabulüne hazırlanan biçim).
//!
//! Yalnız böyle yazıldığında Word'deki görünüşü koruyan tablolar çevrilir:
//! kenarlıksız, en çok 4 sütunlu, birleşik hücresi olmayan, hücrelerinde yalnız
//! paragraf bulunan ve son sütun dışındaki her hücre satırı tek satıra sığan
//! tablolar. Uzun metinli sütunlar, resim, iç içe tablo ya da liste öğesinin
//! etiketle aynı satıra düştüğü durumlar tabloyu olduğu gibi bırakır: o zaman
//! sekmeli satır Word'deki görünüşü bozardı.

use crate::model::*;
use crate::warnings::{Warning, WarningCode, WarningSink};

/// Başlık blokları 2 (etiket, değer) ya da 3 (etiket, ":", değer) sütunludur;
/// tek sütunlu kenarlıksız tablo yalnız bir paragraf kutusudur.
const MAX_COLUMNS: usize = 4;
/// Yazı boyutu bilinmeyen metin için (UDF varsayılanı).
const DEFAULT_SIZE_PT: f32 = 12.0;

/// Gövdedeki çevrilebilir görünmez tabloları sekmeli paragraflara çevirir.
/// `text_width_pt`: sayfanın yazı alanı genişliği.
pub(crate) fn flatten_layout_tables(
    blocks: &mut Vec<Block>,
    text_width_pt: f32,
    warn: &mut WarningSink,
) {
    let mut table_no = 0usize;
    let mut out = Vec::with_capacity(blocks.len());
    for block in std::mem::take(blocks) {
        let Block::Table(t) = block else {
            out.push(block);
            continue;
        };
        table_no += 1;
        match flatten(&t, text_width_pt) {
            Some(paragraphs) => {
                warn.push(
                    Warning::new(WarningCode::LayoutTableFlattened).at(format!("Tablo {table_no}")),
                );
                out.extend(paragraphs.into_iter().map(Block::Paragraph));
            }
            None => out.push(Block::Table(t)),
        }
    }
    *blocks = out;
}

/// Tablonun sekmeli satırları; çevrilemiyorsa `None`.
fn flatten(t: &Table, text_width: f32) -> Option<Vec<Paragraph>> {
    let cols = t.column_widths.len();
    if t.border != TableBorder::None || !(1..=MAX_COLUMNS).contains(&cols) || t.rows.is_empty() {
        return None;
    }
    if t.column_widths.iter().any(|w| !w.is_finite() || *w <= 0.0) {
        return None;
    }
    let indent = t.props.left_indent_pt;
    if indent < -0.5 || indent >= text_width {
        return None; // sol kenar boşluğuna taşan tablo: satır oradan başlayamaz
    }
    // Değer sütunu sağ kenar boşluğuna taşan başlık şablonları yaygın (gerçek
    // belgelerde 465–470 pt'lik tablo, 453 pt'lik yazı alanı). UDF tabloyu zaten
    // oransal yazar ve UYAP yazı alanına sığdırır; satırlar da aynı oranla.
    let total: f32 = t.column_widths.iter().sum();
    // Birkaç punto farkı ölçeklenmez: duraklar Word'deki konumlarında kalır.
    let available = text_width - indent;
    let scale = if total > available + 2.0 {
        available / total
    } else {
        1.0
    };
    // Sütunların kenar boşluğundan ölçülen başlangıçları; son öğe tablonun sağ kenarı.
    let mut edges = Vec::with_capacity(cols + 1);
    let mut x = indent;
    for w in &t.column_widths {
        edges.push(round2(x));
        x += w * scale;
    }
    edges.push(round2(x));

    let mut out = Vec::new();
    for row in &t.rows {
        if row.cells.len() != cols || row.cells.iter().any(|c| c.grid_span != 1) {
            return None;
        }
        let mut cells: Vec<Vec<Paragraph>> = Vec::with_capacity(cols);
        for cell in &row.cells {
            let mut ps = Vec::with_capacity(cell.blocks.len());
            for b in &cell.blocks {
                match b {
                    Block::Paragraph(p) if !p.runs.iter().any(|r| matches!(r, Run::Image(_))) => {
                        ps.extend(visual_lines(p))
                    }
                    _ => return None,
                }
            }
            cells.push(ps);
        }
        let lines = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
        // Dikey ortalanmış bir etiket, çok satırlı bir değerin yanında
        // ortada durur; sekmeli satırda ilk satıra çıkardı.
        if lines > 1
            && row
                .cells
                .iter()
                .any(|c| c.vertical_align != CellVAlign::Top)
        {
            return None;
        }
        for i in 0..lines {
            let segments: Vec<(usize, &Paragraph)> = (0..cols)
                .filter_map(|k| cells[k].get(i).filter(|p| has_ink(p)).map(|p| (k, p)))
                .collect();
            let blank = (0..cols).find_map(|k| cells[k].get(i));
            out.push(line(&segments, blank, &edges, text_width)?);
        }
    }
    Some(out)
}

/// Tablonun bir satırının (hücrelerin i. paragraflarının) tek paragrafı.
fn line(
    segments: &[(usize, &Paragraph)],
    blank: Option<&Paragraph>,
    edges: &[f32],
    text_width: f32,
) -> Option<Paragraph> {
    let line_start = edges[0];
    let Some(&(last_col, last)) = segments.last() else {
        // Hiçbir hücrede bu satırda metin yok: boş satır, aralığıyla.
        let mut props = blank.map(|p| p.props.clone()).unwrap_or_default();
        props.left_indent_pt = line_start;
        props.right_indent_pt = 0.0;
        props.first_line_indent_pt = 0.0;
        props.hanging_pt = 0.0;
        props.tab_stops.clear();
        props.list = None;
        return Some(Paragraph {
            props,
            runs: vec![],
        });
    };
    let right_of = |k: usize| (text_width - edges[k + 1]).max(0.0);

    // Yalnız bir hücrede metin var ve içinde sekme yok: paragraf olduğu gibi,
    // girintileri sütununa kaydırılarak (ortalanmış imza bloğu da böyle kalır).
    if segments.len() == 1 && !has_tab(last) {
        let mut props = last.props.clone();
        props.left_indent_pt += edges[last_col];
        props.right_indent_pt += right_of(last_col);
        return Some(Paragraph {
            props,
            runs: last.runs.clone(),
        });
    }

    // Sekmeli satır. Her parça bir sekmeyle kendi durağına gider; satırın ilk
    // konumu tablonun sol kenarıdır (UDF `LeftIndent`), duraklar kenar
    // boşluğundan ölçülür. UYAP dilekçelerindeki biçimin aynısı.
    let mut stops: Vec<TabStop> = Vec::new();
    let mut runs: Vec<Run> = Vec::new();
    let mut first_line_at = line_start;
    for (n, &(col, p)) in segments.iter().enumerate() {
        let is_last = n + 1 == segments.len();
        // Etiketi metnin önüne koyan liste öğesi tek paragrafta yazılamaz.
        if p.props.list.is_some() {
            return None;
        }
        let wraps = is_last && matches!(p.props.alignment, Alignment::Left | Alignment::Justify);
        if !wraps
            && !fits(
                p,
                edges[col + 1] - edges[col] - first_line_offset(p).max(0.0),
            )
        {
            return None; // sütununa sığmayan parça bir sonraki durağı aşardı
        }
        let (stop, align) = match p.props.alignment {
            Alignment::Left | Alignment::Justify => {
                (edges[col] + first_line_offset(p), TabAlign::Left)
            }
            Alignment::Center => (
                round2((edges[col] + edges[col + 1]) / 2.0),
                TabAlign::Center,
            ),
            Alignment::Right => (edges[col + 1] - p.props.right_indent_pt, TabAlign::Right),
        };
        if n == 0 && col == 0 && align == TabAlign::Left {
            first_line_at = stop; // ilk parça satırın başında, sekmesiz
        } else {
            stops.push(TabStop {
                pos_pt: stop,
                align,
            });
            runs.push(Run::Tab {
                props: first_props(p),
            });
        }
        runs.extend(p.runs.iter().cloned());
    }

    let mut props = last.props.clone();
    props.list = None;
    props.first_line_indent_pt = 0.0;
    props.hanging_pt = 0.0;
    if matches!(last.props.alignment, Alignment::Left | Alignment::Justify) {
        // Değer uzunsa alt satırlar değer sütununun hizasından devam eder.
        props.left_indent_pt = edges[last_col] + last.props.left_indent_pt;
        props.right_indent_pt = right_of(last_col) + last.props.right_indent_pt;
        // Değerin kendi duraklarıyla Word'ün asılı girintideki örtük durağı
        // (madde etiketinden sonraki sekme buraya gider).
        for s in &last.props.tab_stops {
            stops.push(TabStop {
                pos_pt: edges[last_col] + s.pos_pt,
                align: s.align,
            });
        }
        if last.props.hanging_pt > 0.0 {
            stops.push(TabStop {
                pos_pt: props.left_indent_pt,
                align: TabAlign::Left,
            });
        }
    } else {
        props.alignment = Alignment::Left;
        props.left_indent_pt = first_line_at;
        props.right_indent_pt = 0.0;
    }
    if props.left_indent_pt > first_line_at {
        props.hanging_pt = props.left_indent_pt - first_line_at;
    } else {
        props.first_line_indent_pt = first_line_at - props.left_indent_pt;
    }
    stops.sort_by(|a, b| a.pos_pt.total_cmp(&b.pos_pt));
    stops.dedup_by(|a, b| (a.pos_pt - b.pos_pt).abs() < 0.5);
    props.tab_stops = stops;
    Some(Paragraph { props, runs })
}

/// Hücre içi satır sonu (Shift+Enter) ayrı bir görsel satırdır: iki sütunlu
/// imza bloğunda sol ve sağ sütunun aynı sıradaki satırları yan yana durur.
/// (UDF'te paragraf içi satır sonu yok; tek paragrafta kalsa boşluğa dönerdi.)
fn visual_lines(p: &Paragraph) -> Vec<Paragraph> {
    if !p.runs.iter().any(|r| matches!(r, Run::LineBreak { .. })) {
        return vec![p.clone()];
    }
    let mut parts: Vec<Vec<Run>> = vec![Vec::new()];
    for r in &p.runs {
        match r {
            Run::LineBreak { .. } => parts.push(Vec::new()),
            other => parts
                .last_mut()
                .expect("en az bir parça")
                .push(other.clone()),
        }
    }
    let n = parts.len();
    parts
        .into_iter()
        .enumerate()
        .map(|(i, runs)| {
            let mut props = p.props.clone();
            if i > 0 {
                // Satır sonundan sonraki satır gövde girintisinden başlar.
                props.first_line_indent_pt = 0.0;
                props.hanging_pt = 0.0;
                props.space_before_pt = 0.0;
                props.list = None;
            }
            if i + 1 < n {
                props.space_after_pt = 0.0;
            }
            Paragraph { props, runs }
        })
        .collect()
}

/// Konumlar yüzde bir puntoya: ölçekleme "115.78723" gibi duraklar yazmasın.
fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

/// İlk satırın, hücre içeriğinin sol kenarından uzaklığı.
fn first_line_offset(p: &Paragraph) -> f32 {
    p.props.left_indent_pt - p.props.hanging_pt + p.props.first_line_indent_pt
}

/// Paragrafta görünen bir şey var mı (boşluk ve sekme sayılmaz)?
fn has_ink(p: &Paragraph) -> bool {
    p.runs.iter().any(|r| match r {
        Run::Text { text, .. } => text.chars().any(|c| !c.is_whitespace()),
        Run::Image(_) => true,
        Run::Tab { .. } | Run::LineBreak { .. } => false,
    })
}

fn has_tab(p: &Paragraph) -> bool {
    p.runs.iter().any(|r| matches!(r, Run::Tab { .. }))
}

fn first_props(p: &Paragraph) -> RunProps {
    p.runs
        .iter()
        .find_map(|r| match r {
            Run::Text { props, .. } => Some(props.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Parça tek satırda `available` genişliğe sığıyor mu? Ölçü cömert tutulur
/// (Times New Roman'ın büyük ve kalın harfleri geniştir): sığdığından emin
/// olunamayan tablo tablo kalır.
fn fits(p: &Paragraph, available: f32) -> bool {
    let mut width = 0.0;
    for r in &p.runs {
        match r {
            Run::Text { text, props } => {
                let size = props.font_size_pt.unwrap_or(DEFAULT_SIZE_PT);
                let bold = if props.bold { 1.05 } else { 1.0 };
                width += text.chars().map(char_em).sum::<f32>() * size * bold;
            }
            // Sekme ya da satır sonu taşıyan parça bir satıra indirgenemez.
            Run::Tab { .. } | Run::LineBreak { .. } | Run::Image(_) => return false,
        }
    }
    // Sekmenin bir sonraki durağa gitmesi için parça durağın biraz önünde bitmeli.
    width <= available - 2.0
}

/// Bir karakterin yaklaşık genişliği (em), Times New Roman'a göre ve biraz
/// cömert: büyük harf ~0,67, küçük harf ~0,44, rakam 0,5, boşluk 0,25.
fn char_em(c: char) -> f32 {
    if c == ' ' {
        0.25
    } else if c.is_uppercase() {
        0.7
    } else if c.is_ascii_digit() {
        0.5
    } else if c.is_alphabetic() {
        0.47
    } else {
        0.33
    }
}

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
//! Çevrilen tablolar: kenarlıksız, en çok 4 sütunlu ve hücrelerinde yalnız
//! paragraf bulunan tablolar. Birleşik hücre kapladığı sütunların genişliğini alır.
//!
//! Sütununa sığmayan etiket Word'deki gibi kendi sütununda alt satıra iner. Saha:
//! "İSTİNAF EDEN DAVACI (ALACAKLI)" etiketi sütununa sığmadığı için bütün başlık
//! tablo kalıyordu; kullanıcının dilekçelerindeki kenarlıksız tabloların dörtte
//! biri bu yüzden blok oluyordu. Etiket de değer de sarıyorsa değer etiketin son
//! satırından başlar: değer tek paragraf kalır, UYAP'ta düzenlenince akar. Dikey
//! hizalama yok sayılır, etiket ilk satırında durur: blok tablodan iyidir.
//!
//! Tablo olarak kalanlar: kenarlıklı tablo, resim, iç içe tablo, sekme içeren
//! ya da liste öğesi olan etiket ve yan yana iki metin sütunu
//! (etiket sütunu değer sütunundan dar değilken ikisi de sarıyorsa). Bunlarda
//! sekmeli satır Word'deki görünüşü bozardı.

use crate::font_metrics::{char_width, text_width};
use crate::model::*;
use crate::warnings::{Warning, WarningCode, WarningSink};

/// Başlık blokları 2 (etiket, değer) ya da 3 (etiket, ":", değer) sütunludur;
/// tek sütunlu kenarlıksız tablo yalnız bir paragraf kutusudur.
const MAX_COLUMNS: usize = 4;
/// Bir etiketin sarılabileceği en çok satır. Daha uzunu etiket değil metindir.
const MAX_LABEL_LINES: usize = 8;
/// Sekmenin bir sonraki durağa gitmesi için metin durağın biraz önünde bitmeli.
pub(crate) const TAB_CLEARANCE_PT: f32 = 2.0;

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
        // Her hücrenin yatay sınırları; birleşik hücre kapladığı sütunların
        // tamamıdır (başlık satırı bütün tabloyu kaplayabilir).
        let mut bounds = Vec::with_capacity(row.cells.len());
        let mut grid = 0;
        for cell in &row.cells {
            let span = cell.grid_span.max(1);
            if grid + span > cols {
                return None;
            }
            bounds.push((edges[grid], edges[grid + span]));
            grid += span;
        }
        if bounds.is_empty() {
            return None;
        }
        let mut cells: Vec<Vec<Paragraph>> = Vec::with_capacity(row.cells.len());
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
        wrap_labels(&mut cells, &bounds)?;
        let lines = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
        for i in 0..lines {
            let segments: Vec<(usize, &Paragraph)> = (0..cells.len())
                .filter_map(|k| cells[k].get(i).filter(|p| has_ink(p)).map(|p| (k, p)))
                .collect();
            let blank = (0..cells.len()).find_map(|k| cells[k].get(i));
            out.push(line(&segments, blank, &bounds, edges[0], text_width)?);
        }
    }
    Some(out)
}

/// Satırın son dolu hücresinden önceki hücrelerde sütununa sığmayan paragraf,
/// Word'deki gibi sütunun içinde alt satırlara bölünür. `None`: bölünemiyor ya da
/// satır sekmeli satırla gösterilemiyor, tablo kalmalı.
fn wrap_labels(cells: &mut [Vec<Paragraph>], bounds: &[(f32, f32)]) -> Option<()> {
    let Some(last) = cells.iter().rposition(|ps| ps.iter().any(has_ink)) else {
        return Some(());
    };
    let column = |k: usize| bounds[k].1 - bounds[k].0;
    let mut wrapped = Vec::new();
    for k in 0..last {
        // Etiketin sonundaki boşluklar Word'de hücrenin içinde görünmez; sekmeli
        // satırda ise sekmeyi bir sonraki durağa itiyordu ("Tel No       ").
        cells[k].iter_mut().for_each(trim_trailing_spaces);
        // Yalnız yanında başka hücrenin metni olan satır bölünür; yanı boş
        // satır kendi sütununda tek paragraf olarak sarar.
        let partners = cells[k + 1..=last].iter().map(Vec::len).max().unwrap_or(0);
        let mut lines = Vec::with_capacity(cells[k].len() + 1);
        let mut did = false;
        for p in std::mem::take(&mut cells[k]) {
            if !has_ink(&p)
                || lines.len() >= partners
                || fits(&p, column(k) - first_line_offset(&p).max(0.0))
            {
                lines.push(p);
            } else {
                let indent = first_line_offset(&p).max(p.props.left_indent_pt).max(0.0);
                lines.extend(wrap(&p, column(k) - indent - TAB_CLEARANCE_PT)?);
                did = true;
            }
        }
        cells[k] = lines;
        if did {
            wrapped.push(k);
        }
    }
    let value_wraps = cells[last].iter().any(|p| {
        has_ink(p)
            && !fits(
                p,
                column(last) - first_line_offset(p).max(0.0) - p.props.right_indent_pt,
            )
    });
    if wrapped.is_empty() || !value_wraps {
        return Some(());
    }
    // Yan yana iki metin sütunu sekmeli satırla gösterilemez.
    if wrapped.iter().any(|&k| column(k) >= column(last)) {
        return None;
    }
    // Değer tek paragraf kalsın diye etiketin son satırından başlar; önceki
    // hücreler (etiket, ":") son satırlarına hizalanır.
    let head = cells[..last].iter().map(Vec::len).max().unwrap_or(1);
    for (k, cell) in cells.iter_mut().enumerate().take(last + 1) {
        let pad = if k == last {
            head - 1
        } else {
            head - cell.len()
        };
        cell.splice(0..0, std::iter::repeat(Paragraph::default()).take(pad));
    }
    Some(())
}

fn trim_trailing_spaces(p: &mut Paragraph) {
    while let Some(Run::Text { text, .. }) = p.runs.last_mut() {
        let kept = text.trim_end_matches([' ', '\u{a0}']).len();
        text.truncate(kept);
        if !text.is_empty() {
            break;
        }
        p.runs.pop();
    }
}

/// Paragrafı, her satırı `limit` genişliğe sığan satırlara böler: sözcük
/// sınırında, tek başına sığmayan sözcükte harf sınırında (Word hücrede böyle
/// sarar). Sekme ya da görsel taşıyan paragraf bölünmez.
fn wrap(p: &Paragraph, limit: f32) -> Option<Vec<Paragraph>> {
    let mut chars: Vec<(usize, char)> = Vec::new();
    for (i, r) in p.runs.iter().enumerate() {
        let Run::Text { text, .. } = r else {
            return None;
        };
        chars.extend(text.chars().map(|c| (i, c)));
    }
    let props_of = |i: usize| match &p.runs[i] {
        Run::Text { props, .. } => props,
        _ => unreachable!("yalnız metin parçaları"),
    };
    let n = chars.len();
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < n {
        let (mut width, mut i, mut space) = (0.0, start, None);
        while i < n {
            let (run, c) = chars[i];
            let w = char_width(c, props_of(run));
            if c == ' ' {
                space = Some(i);
            } else if width + w > limit && i > start {
                break;
            }
            width += w;
            i += 1;
        }
        if i == n {
            ranges.push(start..n);
            break;
        }
        let end = match space {
            Some(s) if s > start => s,
            _ => i,
        };
        ranges.push(start..end);
        start = end;
        while start < n && chars[start].1 == ' ' {
            start += 1;
        }
    }
    if ranges.len() > MAX_LABEL_LINES {
        return None;
    }
    let count = ranges.len();
    Some(
        ranges
            .into_iter()
            .enumerate()
            .map(|(line, range)| {
                let mut runs: Vec<Run> = Vec::new();
                let mut current = usize::MAX;
                for &(run, c) in &chars[range] {
                    match runs.last_mut() {
                        Some(Run::Text { text, .. }) if run == current => text.push(c),
                        _ => runs.push(Run::Text {
                            text: c.to_string(),
                            props: props_of(run).clone(),
                        }),
                    }
                    current = run;
                }
                let mut props = p.props.clone();
                if line > 0 {
                    props.first_line_indent_pt = 0.0;
                    props.hanging_pt = 0.0;
                    props.space_before_pt = 0.0;
                    props.list = None;
                }
                if line + 1 < count {
                    props.space_after_pt = 0.0;
                }
                Paragraph { props, runs }
            })
            .collect(),
    )
}

/// Tablonun bir satırının (hücrelerin i. paragraflarının) tek paragrafı.
/// `bounds`: satırın hücrelerinin kenar boşluğundan ölçülen sol ve sağ kenarı;
/// `line_start`: tablonun sol kenarı.
fn line(
    segments: &[(usize, &Paragraph)],
    blank: Option<&Paragraph>,
    bounds: &[(f32, f32)],
    line_start: f32,
    text_width: f32,
) -> Option<Paragraph> {
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
    let right_of = |k: usize| (text_width - bounds[k].1).max(0.0);

    // Yalnız bir hücrede metin var ve içinde sekme yok: paragraf olduğu gibi,
    // girintileri sütununa kaydırılarak (ortalanmış imza bloğu da böyle kalır).
    if segments.len() == 1 && !has_tab(last) {
        let mut props = last.props.clone();
        props.left_indent_pt += bounds[last_col].0;
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
                bounds[col].1 - bounds[col].0 - first_line_offset(p).max(0.0),
            )
        {
            return None; // sütununa sığmayan parça bir sonraki durağı aşardı
        }
        let (stop, align) = match p.props.alignment {
            Alignment::Left | Alignment::Justify => {
                (bounds[col].0 + first_line_offset(p), TabAlign::Left)
            }
            Alignment::Center => (
                round2((bounds[col].0 + bounds[col].1) / 2.0),
                TabAlign::Center,
            ),
            Alignment::Right => (bounds[col].1 - p.props.right_indent_pt, TabAlign::Right),
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
        props.left_indent_pt = bounds[last_col].0 + last.props.left_indent_pt;
        props.right_indent_pt = right_of(last_col) + last.props.right_indent_pt;
        // Değerin kendi duraklarıyla Word'ün asılı girintideki örtük durağı
        // (madde etiketinden sonraki sekme buraya gider).
        for s in &last.props.tab_stops {
            stops.push(TabStop {
                pos_pt: bounds[last_col].0 + s.pos_pt,
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
            Run::Text { text, props } => width += text_width(text, props),
            // Sekme ya da satır sonu taşıyan parça bir satıra indirgenemez.
            Run::Tab { .. } | Run::LineBreak { .. } | Run::Image(_) => return false,
        }
    }
    width <= available - TAB_CLEARANCE_PT
}

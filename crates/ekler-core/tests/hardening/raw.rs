//! Bayt bayt PDF kurucusu: klasik xref, artımlı güncelleme, xref akışı ve nesne
//! akışı. lopdf 0.34 nesne akışı yazmadığı için bu düzenler elle kurulur.

use std::collections::BTreeMap;
use std::io::Write;

pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

pub struct RawPdf {
    pub bytes: Vec<u8>,
    pub offsets: BTreeMap<u32, usize>,
}

impl RawPdf {
    pub fn new(version: &str) -> Self {
        let mut bytes = format!("%PDF-{version}\n").into_bytes();
        bytes.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");
        Self {
            bytes,
            offsets: BTreeMap::new(),
        }
    }

    pub fn object(&mut self, id: u32, body: &[u8]) {
        self.offsets.insert(id, self.bytes.len());
        self.bytes
            .extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        self.bytes.extend_from_slice(body);
        self.bytes.extend_from_slice(b"\nendobj\n");
    }

    pub fn stream(&mut self, id: u32, dict: &str, data: &[u8]) {
        self.stream_with_length(id, dict, data, data.len());
    }

    /// `/Length` bilerek yanlış yazılabilsin diye ayrı.
    pub fn stream_with_length(&mut self, id: u32, dict: &str, data: &[u8], length: usize) {
        let mut body = format!("<<{dict}/Length {length}>>stream\n").into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.object(id, &body);
    }

    pub fn startxref(&mut self, at: usize) {
        self.bytes
            .extend_from_slice(format!("startxref\n{at}\n%%EOF\n").as_bytes());
    }

    /// 0..size klasik tablo; `live` dışındakiler serbest.
    pub fn classic_table(&mut self, size: u32, live: &[u32], trailer: &str) -> usize {
        let at = self.bytes.len();
        let mut x = format!("xref\n0 {size}\n");
        for n in 0..size {
            match self.offsets.get(&n).filter(|_| live.contains(&n)) {
                Some(off) => x.push_str(&format!("{off:010} 00000 n \n")),
                None => x.push_str("0000000000 65535 f \n"),
            }
        }
        x.push_str(&format!("trailer\n<<{trailer}>>\n"));
        self.bytes.extend_from_slice(x.as_bytes());
        self.startxref(at);
        at
    }

    /// Bütün yazılmış nesnelerle klasik tablo.
    pub fn finish_classic(&mut self, trailer: &str) -> usize {
        let size = self.offsets.keys().max().copied().unwrap_or(0) + 1;
        let live: Vec<u32> = self.offsets.keys().copied().collect();
        self.classic_table(size, &live, &format!("/Size {size}{trailer}"))
    }

    /// Artımlı güncelleme bölümü: yalnız değişen nesneler.
    pub fn update_table(&mut self, changed: &[u32], trailer: &str) -> usize {
        let at = self.bytes.len();
        let mut x = String::from("xref\n0 1\n0000000000 65535 f \n");
        for id in changed {
            x.push_str(&format!("{id} 1\n{:010} 00000 n \n", self.offsets[id]));
        }
        x.push_str(&format!("trailer\n<<{trailer}>>\n"));
        self.bytes.extend_from_slice(x.as_bytes());
        self.startxref(at);
        at
    }

    /// W[1 3 1] xref akışı. `rows`: (nesne, tür, alan2, alan3), artan sırada.
    pub fn xref_stream(&mut self, id: u32, size: u32, rows: &[(u32, u8, u32, u8)], extra: &str) {
        let mut index: Vec<(u32, u32)> = Vec::new();
        let mut raw = Vec::new();
        for &(n, kind, f2, f3) in rows {
            match index.last_mut() {
                Some((start, len)) if *start + *len == n => *len += 1,
                _ => index.push((n, 1)),
            }
            raw.extend_from_slice(&[kind, (f2 >> 16) as u8, (f2 >> 8) as u8, f2 as u8, f3]);
        }
        let index: String = index.iter().map(|(s, l)| format!("{s} {l} ")).collect();
        self.stream(
            id,
            &format!("/Type/XRef/Size {size}/W[1 3 1]/Index[{index}]/Filter/FlateDecode{extra}"),
            &zlib(&raw),
        );
    }

    /// Nesne akışı: `members` sırasıyla.
    pub fn object_stream(&mut self, id: u32, members: &[(u32, String)]) {
        let (mut header, mut body) = (String::new(), String::new());
        for (n, obj) in members {
            header.push_str(&format!("{n} {} ", body.len()));
            body.push_str(obj);
            body.push('\n');
        }
        self.stream(
            id,
            &format!(
                "/Type/ObjStm/N {}/First {}/Filter/FlateDecode",
                members.len(),
                header.len()
            ),
            &zlib(format!("{header}{body}").as_bytes()),
        );
    }
}

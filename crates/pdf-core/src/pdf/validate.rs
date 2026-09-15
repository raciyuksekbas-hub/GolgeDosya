use crate::{EklerError, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashSet;
fn invalid(message: impl Into<String>) -> EklerError {
    EklerError::InvalidPdf(message.into())
}

/// Sayfa görünüm grafiğinde bile izlenmeyen, çizimi değiştirmeyen anahtarlar:
/// meta veri, uygulamaya özel veri, metin çıkarma eşlemesi (`/ToUnicode`),
/// font alt küme bilgisi (`/CIDSet`), yapı ağacı bağları. Yalnız anahtarları
/// spec'in tanımladığı yapısal sözlüklerde uygulanır.
const OUTSIDE_APPEARANCE: &[&[u8]] = &[
    b"Metadata",
    b"PieceInfo",
    b"ToUnicode",
    b"CIDSet",
    b"StructParent",
    b"StructParents",
];

/// Değeri, anahtarlarını yazarın seçtiği bir sözlük olan anahtarlar: kaynak
/// haritaları ve Type3 `/CharProcs`. Böyle bir sözlükte `/P`, `/A` ya da
/// `/Metadata` adlı bir font ya da glif olabilir; her girdisi izlenir.
const NAME_MAPS: &[&[u8]] = &[
    b"Font",
    b"XObject",
    b"ExtGState",
    b"ColorSpace",
    b"Pattern",
    b"Shading",
    b"Properties",
    b"CharProcs",
];

/// Eksik nesne başvurularını ONARILAMAZ bozulma ile TOLERE EDİLEBİLİR
/// düzensizlik olarak ayırır.
///
/// - Sayfa ağacında (`/Kids`), bir sayfanın içeriğinde (`/Contents`), miras
///   dâhil kaynaklarında (`/Resources` ve ondan erişilen her şey: font, gömülü
///   font programı, Type3 glifi, görsel, SMask, ICC, Form XObject, ExtGState,
///   desen, gölgeleme), saydamlık grubunda ve sayfa kutularında eksik nesne ya
///   da Catalog `/OCProperties`in (hangi katmanın görüneceği) eksik nesnesi:
///   belge güvenle gösterilemez, hata.
/// - Bunun DIŞINDA eksik nesne — açıklama ve görünüm akışı, popup, eylem,
///   form alanı, yapı ağacı, anahat, ad ağacı, XMP, `/ToUnicode` —
///   ISO 32000-1 §7.3.10'a göre hata değildir, null sayılır: sözlük girdisi
///   silinir, dizi öğesi ya da tümüyle başvuru olan nesne `null` olur.
///   Örnek: macOS 26 PDFKit'in yeniden kaydettiği etiketli belgede
///   `/StructTreeRoot /IDTree 71 0 R` yazılı, 71 numaralı nesne dosyada yok.
///
/// Dönen sayı indirgenen başvuru sayısıdır. Çağıran ardından
/// [`validate_document`] ile belgeyi katı kurallarla yeniden doğrular.
pub(crate) fn detach_dangling_references(doc: &mut Document) -> Result<usize> {
    require_complete_page_graph(doc)?;
    let existing: HashSet<ObjectId> = doc.objects.keys().copied().collect();
    let mut detached = 0;
    for object in doc.objects.values_mut() {
        detached += match object {
            Object::Reference(id) if !existing.contains(id) => {
                *object = Object::Null;
                1
            }
            other => detach_in_object(other, &existing),
        };
    }
    detached += detach_in_dictionary(&mut doc.trailer, &existing);
    Ok(detached)
}

/// Hata sözündeki yer.
#[derive(Clone, Copy)]
enum Place {
    Page(u32),
    Inherited,
    OptionalContent,
}

fn require_complete_page_graph(doc: &Document) -> Result<()> {
    let Some(catalog) = doc
        .trailer
        .get(b"Root")
        .and_then(|r| r.as_reference())
        .ok()
        .and_then(|r| doc.get_dictionary(r).ok())
    else {
        // Catalog yoksa hatayı validate_document kendi sözüyle verir.
        return Ok(());
    };
    let mut budget = 2_000_000usize;
    let mut spend = || {
        budget = budget
            .checked_sub(1)
            .ok_or_else(|| invalid("PDF nesne sınırı aşıldı"))?;
        Ok::<(), EklerError>(())
    };
    let numbers: std::collections::HashMap<ObjectId, u32> =
        doc.get_pages().into_iter().map(|(n, id)| (id, n)).collect();
    // Görünüm grafiği yığını: (yer, ad haritası mı, nesne).
    let mut stack: Vec<(Place, bool, Object)> = Vec::new();
    let push = |stack: &mut Vec<(Place, bool, Object)>,
                place: Place,
                node: &Dictionary,
                keys: &[&[u8]]| {
        for key in keys {
            if let Ok(value) = node.get(key) {
                stack.push((place, false, value.clone()));
            }
        }
    };
    const INHERITABLE: &[&[u8]] = &[b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
    const PAGE_KEYS: &[&[u8]] = &[
        b"Contents",
        b"Resources",
        b"Group",
        b"MediaBox",
        b"CropBox",
        b"Rotate",
    ];

    // 1) Sayfa ağacı: her düğüm var olmalı. Her düğümün KENDİ görünüm girdileri
    //    yığına bir kez girer; miras alınan doğrudan bir kaynak sözlüğü sayfa
    //    başına çoğaltılmaz (2.100 sayfa × 1.000 girdilik miras harita bütçeyi
    //    aşıyor, belleği şişiriyordu).
    let mut seen = HashSet::new();
    let mut root = None;
    let mut nodes: Vec<Object> = catalog.get(b"Pages").ok().cloned().into_iter().collect();
    while let Some(node) = nodes.pop() {
        spend()?;
        match node {
            Object::Reference(id) => {
                if !seen.insert(id) {
                    continue;
                }
                let found = doc
                    .get_object(id)
                    .map_err(|_| invalid(format!("Eksik nesne referansı: {id:?} (sayfa ağacı)")))?;
                match found {
                    Object::Dictionary(d) => match d.get(b"Type").and_then(|t| t.as_name()) {
                        Ok(b"Pages") => {
                            root.get_or_insert(id);
                            push(&mut stack, Place::Inherited, d, INHERITABLE);
                            nodes.extend(d.get(b"Kids").ok().cloned());
                        }
                        Ok(b"Page") => {
                            let place = Place::Page(numbers.get(&id).copied().unwrap_or(0));
                            push(&mut stack, place, d, PAGE_KEYS);
                        }
                        _ => {}
                    },
                    other => nodes.push(other.clone()),
                }
            }
            Object::Array(items) => nodes.extend(items),
            _ => {}
        }
    }
    // 2) Kök düğümün üstündeki /Parent zinciri de miras verir (görüntüleyiciler
    //    zinciri sonuna kadar yürür). Dosyada olmayan üst düğüm null sayılır ve
    //    zincir orada biter; sayfanın kendi içeriği yine yukarıda denetlendi.
    let mut above = root
        .and_then(|id| doc.get_dictionary(id).ok())
        .and_then(|d| d.get(b"Parent").ok())
        .and_then(|p| p.as_reference().ok());
    while let Some(id) = above {
        spend()?;
        if !seen.insert(id) {
            break;
        }
        let Ok(node) = doc.get_dictionary(id) else {
            break;
        };
        push(&mut stack, Place::Inherited, node, INHERITABLE);
        above = node.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    // 3) İsteğe bağlı içeriğin yapılandırması, sayfada hangi katmanın
    //    görüneceğini belirler.
    if let Ok(oc) = catalog.get(b"OCProperties") {
        stack.push((Place::OptionalContent, false, oc.clone()));
    }

    let mut seen = HashSet::new();
    while let Some((place, name_map, object)) = stack.pop() {
        spend()?;
        let found = match &object {
            Object::Reference(id) => {
                if !seen.insert(*id) {
                    continue;
                }
                doc.get_object(*id).map_err(|_| {
                    let place = match place {
                        Place::Page(0) => "sayfa içeriği ya da kaynakları".to_string(),
                        Place::Page(n) => format!("sayfa {n} içeriği ya da kaynakları"),
                        Place::Inherited => "sayfa ağacından miras alınan kaynaklar".into(),
                        Place::OptionalContent => "isteğe bağlı içerik yapılandırması".into(),
                    };
                    invalid(format!("Eksik nesne referansı: {id:?} ({place})"))
                })?
            }
            direct => direct,
        };
        match found {
            Object::Array(items) => stack.extend(items.iter().map(|i| (place, false, i.clone()))),
            Object::Dictionary(dict) => push_appearance_values(&mut stack, place, name_map, dict),
            Object::Stream(stream) => {
                push_appearance_values(&mut stack, place, name_map, &stream.dict)
            }
            Object::Reference(_) => stack.push((place, name_map, found.clone())),
            _ => {}
        }
    }
    Ok(())
}

fn push_appearance_values(
    stack: &mut Vec<(Place, bool, Object)>,
    place: Place,
    name_map: bool,
    dict: &Dictionary,
) {
    for (key, value) in dict.iter() {
        if name_map {
            stack.push((place, false, value.clone()));
        } else if !OUTSIDE_APPEARANCE.contains(&key.as_slice()) {
            stack.push((place, NAME_MAPS.contains(&key.as_slice()), value.clone()));
        }
    }
}

fn detach_in_object(object: &mut Object, existing: &HashSet<ObjectId>) -> usize {
    match object {
        Object::Array(items) => items
            .iter_mut()
            .map(|item| match item {
                Object::Reference(id) if !existing.contains(id) => {
                    *item = Object::Null;
                    1
                }
                other => detach_in_object(other, existing),
            })
            .sum(),
        Object::Dictionary(dict) => detach_in_dictionary(dict, existing),
        Object::Stream(stream) => detach_in_dictionary(&mut stream.dict, existing),
        _ => 0,
    }
}

fn detach_in_dictionary(dict: &mut Dictionary, existing: &HashSet<ObjectId>) -> usize {
    let dangling: Vec<Vec<u8>> = dict
        .iter()
        .filter(|(_, v)| matches!(v, Object::Reference(id) if !existing.contains(id)))
        .map(|(k, _)| k.clone())
        .collect();
    for key in &dangling {
        dict.remove(key);
    }
    dangling.len()
        + dict
            .iter_mut()
            .map(|(_, v)| detach_in_object(v, existing))
            .sum::<usize>()
}
/// Validate reachable references and the full page tree before trusting get_pages.
pub fn validate_document(doc: &Document) -> Result<usize> {
    if doc.trailer.has(b"Encrypt") {
        return Err(invalid(
            "Şifreli PDF desteklenmiyor; kaynak uygulamada şifresiz bir kopya oluşturun",
        ));
    }
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|_| invalid("Catalog referansı yok"))?;
    let catalog = doc
        .get_dictionary(root)
        .map_err(|_| invalid("Catalog okunamıyor"))?;
    if catalog.get(b"Type").and_then(|v| v.as_name()).ok() != Some(b"Catalog") {
        return Err(invalid("Geçersiz Catalog"));
    }
    let mut visited = HashSet::new();
    let mut stack = vec![Object::Reference(root)];
    let mut budget = 2_000_000usize;
    while let Some(object) = stack.pop() {
        budget = budget
            .checked_sub(1)
            .ok_or_else(|| invalid("PDF nesne sınırı aşıldı"))?;
        match object {
            Object::Reference(id) => {
                if visited.insert(id) {
                    stack.push(
                        doc.get_object(id)
                            .map_err(|_| invalid(format!("Eksik nesne referansı: {:?}", id)))?
                            .clone(),
                    );
                }
            }
            Object::Array(a) => stack.extend(a),
            Object::Dictionary(d) => stack.extend(d.iter().map(|(_, v)| v.clone())),
            Object::Stream(s) => {
                validate_flate_stream(&s)?;
                stack.extend(s.dict.iter().map(|(_, v)| v.clone()));
            }
            _ => {}
        }
    }
    let pages = catalog
        .get(b"Pages")
        .and_then(|o| o.as_reference())
        .map_err(|_| invalid("Pages referansı yok"))?;
    fn walk(
        doc: &Document,
        id: ObjectId,
        parent: Option<ObjectId>,
        seen: &mut HashSet<ObjectId>,
        depth: usize,
    ) -> Result<usize> {
        if depth > 128 || !seen.insert(id) {
            return Err(invalid("Döngüsel veya mükerrer sayfa ağacı"));
        }
        let node = doc
            .get_dictionary(id)
            .map_err(|_| invalid("Sayfa ağacı düğümü geçersiz"))?;
        if let Some(parent) = parent {
            if node.get(b"Parent").and_then(|v| v.as_reference()).ok() != Some(parent) {
                return Err(invalid("Sayfa Parent ilişkisi tutarsız"));
            }
        }
        match node
            .get(b"Type")
            .and_then(|v| v.as_name())
            .map_err(|_| invalid("Sayfa Type eksik"))?
        {
            b"Page" => {
                super::resolved_page_dictionary(doc, id)?;
                if let Ok(contents) = node.get(b"Contents") {
                    let check = |v: &Object| -> Result<()> {
                        let id = v
                            .as_reference()
                            .map_err(|_| invalid("Contents referans olmalı"))?;
                        doc.get_object(id)
                            .and_then(|o| o.as_stream())
                            .map_err(|_| invalid("Contents stream eksik"))?;
                        Ok(())
                    };
                    // `/Contents` değeri ISO 32000-1 §7.7.3.3 gereği DOLAYLI
                    // olabilir ve bir akışa ya da akış dizisine çözülür. Dizi mi
                    // tek akış mı kararından ÖNCE dereference et; yoksa diziye
                    // çözülen dolaylı bir başvuru (`/Contents 5 0 R`, 5 = `[6 0 R]`)
                    // tek stream sanılıp reddedilir — her görüntüleyici açsa da.
                    let resolved = doc.dereference(contents).map(|(_, o)| o).unwrap_or(contents);
                    match resolved {
                        Object::Array(a) => {
                            for v in a {
                                check(v)?
                            }
                        }
                        // Dolaylı başvuru doğrudan bir akışa çözüldü: geçerli.
                        Object::Stream(_) => {}
                        v => check(v)?,
                    }
                }
                Ok(1)
            }
            b"Pages" => {
                let kids = node
                    .get(b"Kids")
                    .and_then(|v| v.as_array())
                    .map_err(|_| invalid("Kids eksik"))?;
                let mut count = 0;
                for kid in kids {
                    count += walk(
                        doc,
                        kid.as_reference()
                            .map_err(|_| invalid("Geçersiz Kids referansı"))?,
                        Some(id),
                        seen,
                        depth + 1,
                    )?;
                }
                if node.get(b"Count").and_then(|v| v.as_i64()).ok() != Some(count as i64) {
                    return Err(invalid("Pages Count tutarsız"));
                }
                Ok(count)
            }
            _ => Err(invalid("Bilinmeyen sayfa düğümü")),
        }
    }
    let count = walk(doc, pages, None, &mut HashSet::new(), 0)?;
    if count == 0 {
        return Err(invalid("PDF hiç sayfa içermiyor"));
    }
    Ok(count)
}

/// lopdf 0.34 logs certain zlib errors and returns partial data. Never accept
/// those errors as a successful repair. This is a bounded validation pass only.
fn validate_flate_stream(stream: &lopdf::Stream) -> Result<()> {
    use std::io::Read;
    let single_flate = match stream.dict.get(b"Filter") {
        Ok(Object::Name(n)) => n == b"FlateDecode" || n == b"Fl",
        Ok(Object::Array(a)) if a.len() == 1 => a[0].as_name().ok() == Some(b"FlateDecode"),
        _ => false,
    };
    if !single_flate {
        return Ok(());
    }
    if stream.content.len() < 6 {
        return Err(invalid("Eksik Flate akışı"));
    }
    let mut decoder = flate2::read::ZlibDecoder::new(stream.content.as_slice());
    let mut buffer = [0u8; 65536];
    let mut total = 0usize;
    loop {
        let read = decoder
            .read(&mut buffer)
            .map_err(|_| invalid("Bozuk sıkıştırılmış PDF akışı; kısmi içerik kabul edilmedi"))?;
        if read == 0 {
            break;
        }
        total += read;
        if total > 128 * 1024 * 1024 {
            return Err(invalid("PDF akışının açılmış boyutu güvenli sınırı aşıyor"));
        }
    }
    // Akış temiz çözüldü: döngü hatasız `read == 0` (zlib EOD) ile bitti.
    // lopdf 0.34'ün hatalı zlib'de kısmi veri döndürmesi endişesi YUKARIDAKİ
    // read hatasında (kısmi içerik reddi) yakalanır; yarıda kesilmiş bir akış
    // temiz EOD değil, çözme hatası üretir. Buraya ulaşan her şey geçerli ve
    // eksiksiz bir zlib akışıdır.
    //
    // `total_in` çoğu zaman `content.len()`e eşittir, ama pek çok üretici
    // `endstream`den önceki satır sonunu (LF/CRLF) ya da hizalama dolgusunu
    // `/Length`e katar. Bu baytlar zlib EOD'undan SONRADIR, çözülen içeriğe
    // girmez ve ISO 32000-1 gereği her görüntüleyici yok sayar; onlar yüzünden
    // belge reddedilmez (kendi görsel yolumuz da bunu tolere eder, optimizer.rs).
    // Eşitlik dayatmak, görüntüleyicinin sorunsuz açtığı belgeyi kilitliyordu.
    // Tutarsızlık yalnızca çözücünün var olandan FAZLA girdi tüketmesidir —
    // mantıken imkânsız; yine de savunma amaçlı denetlenir.
    if decoder.total_in() > stream.content.len() as u64 {
        return Err(invalid("Sıkıştırılmış PDF akışı uzunluğu tutarsız"));
    }
    Ok(())
}

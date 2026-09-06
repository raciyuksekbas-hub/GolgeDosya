//! Feature-level rollback (talep §17).
//!
//! Her modülün iki kapısı var:
//!
//! * **Derleme zamanı** — cargo feature. Kapalıysa motor binary'ye hiç linklenmez.
//! * **Çalışma zamanı** — `BELGE_DISABLE_<AD>` ortam değişkeni. Derlenmiş bir
//!   modülü yeniden derlemeden kapatmaya yarar; bir kullanıcıda sorun çıktığında
//!   sürüm geri almadan önce denenecek ilk şey budur.
//!
//! Arayüz rotayı yalnız ilgili feature açıkken gösterir. Bir modülün kapanması
//! diğerlerini etkilemez; DüzenEk sorun çıkarırsa yalnız "Düzenle" kaybolur.
//!
//! Modül anahtarları **eski ürün adlarını** kullanır. Kullanıcı arayüzü Türkçe
//! eylem adlarını gösterir (Düzenle/Dönüştür/Karşılaştır/Denetle) ama log, hata
//! ve feature flag isimleri migration boyunca eski adlarda kalır: bir kayıtta
//! "duzenek" görmek, hangi kod tabanından geldiğini tartışmasız yapar.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureState {
    /// Kararlı anahtar. UI etiketi değil.
    pub key: &'static str,
    /// Kabuğun gösterdiği Türkçe eylem adı.
    pub label: &'static str,
    /// Rota yolu.
    pub route: &'static str,
    /// Motor bu binary'ye linklendi mi?
    pub compiled: bool,
    /// Çalışma zamanında açık mı?
    pub enabled: bool,
}

const MODULES: [(&str, &str, &str); 4] = [
    ("duzenek", "Düzenle", "/duzenle"),
    ("tavzih", "Dönüştür", "/donustur"),
    ("degisikis", "Karşılaştır", "/karsilastir"),
    ("ikincigoz", "Denetle", "/denetle"),
];

fn compiled(key: &str) -> bool {
    match key {
        "duzenek" => cfg!(feature = "feature_duzenek"),
        "tavzih" => cfg!(feature = "feature_tavzih"),
        "degisikis" => cfg!(feature = "feature_degisikis"),
        "ikincigoz" => cfg!(feature = "feature_ikincigoz"),
        _ => false,
    }
}

fn disabled_at_runtime(key: &str) -> bool {
    let var = format!("BELGE_DISABLE_{}", key.to_ascii_uppercase());
    matches!(
        std::env::var(var).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

pub fn states() -> Vec<FeatureState> {
    MODULES
        .iter()
        .map(|(key, label, route)| {
            let c = compiled(key);
            FeatureState {
                key,
                label,
                route,
                compiled: c,
                enabled: c && !disabled_at_runtime(key),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_declares_exactly_the_four_modules_in_navigation_order() {
        let s = states();
        assert_eq!(s.len(), 4);
        assert_eq!(
            s.iter().map(|f| f.label).collect::<Vec<_>>(),
            vec!["Düzenle", "Dönüştür", "Karşılaştır", "Denetle"]
        );
        assert_eq!(
            s.iter().map(|f| f.key).collect::<Vec<_>>(),
            vec!["duzenek", "tavzih", "degisikis", "ikincigoz"]
        );
    }

    #[test]
    fn nothing_is_enabled_before_its_engine_is_compiled_in() {
        // Phase 2: hiçbir modül taşınmadı, dolayısıyla hiçbiri derlenmedi.
        for f in states() {
            if !f.compiled {
                assert!(!f.enabled, "{} linklenmemişken açık görünüyor", f.key);
            }
        }
    }

    #[test]
    fn routes_are_unique_and_rooted() {
        let s = states();
        let mut routes: Vec<_> = s.iter().map(|f| f.route).collect();
        routes.sort();
        routes.dedup();
        assert_eq!(routes.len(), 4);
        assert!(s.iter().all(|f| f.route.starts_with('/')));
    }
}

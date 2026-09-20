//! Ekler arayüzünün motora GERÇEKTEN geçirebildiği `Project` sözleşmesi.
//!
//! SAHA TURU / ÇEKİŞMELİ İNCELEME: yeni Ekler çalışma alanı `Project`'i
//! elle kuruyordu ve iki alanı yanlış dolduruyordu. Yüzey testi yalnız
//! KAYNAK METNİ arıyordu (`invoke("duzenek_prepare_uyap")` yazılmış mı),
//! dolayısıyla "Ekleri Hazırla ve Kaydet" her çalıştırmada düşerken test
//! yeşil kalıyordu.
//!
//! Bu test metni değil SÖZLEŞMEYİ sınar: arayüzün gönderdiği JSON
//! gerçekten `Project`'e çözülüyor mu ve motorun doğrulamasını geçiyor mu?
use ekler_core::model::{Project, DEFAULT_TARGET_SIZE_BYTES};

/// Arayüzün gönderdiği gövdenin aynısı (bkz. `ekler/AnnexWorkspace.tsx`).
fn ui_payload(stamp: &str, target: u64) -> String {
    format!(
        r#"{{
            "version": "1.0.0",
            "name": "Dilekçe Ekleri",
            "created_at": "",
            "updated_at": "",
            "sources": [],
            "exhibits": [],
            "target_size_bytes": {target},
            "stamp_config": {stamp}
        }}"#
    )
}

#[test]
fn a_null_stamp_config_cannot_reach_the_engine() {
    // `StampConfig` düz bir struct alanıdır (Option değil, serde default yok),
    // dolayısıyla `null` daha komut gövdesine girmeden reddedilir.
    let err = serde_json::from_str::<Project>(&ui_payload("null", DEFAULT_TARGET_SIZE_BYTES))
        .expect_err("null stamp_config çözülmemeli");
    assert!(
        err.to_string().contains("invalid type: null"),
        "beklenmeyen hata: {err}"
    );
}

#[test]
fn the_payload_the_ui_sends_deserialises() {
    // Arayüz artık tam bir `StampConfig` gönderir.
    let stamp = r#"{"enabled":true,"position":"bottom_right","font_size":9.0,"margin_pt":18.0,"show_badge":true}"#;
    let project: Project = serde_json::from_str(&ui_payload(stamp, DEFAULT_TARGET_SIZE_BYTES))
        .expect("arayüzün gövdesi Project'e çözülmeli");
    assert_eq!(project.target_size_bytes, DEFAULT_TARGET_SIZE_BYTES);
    assert!(project.stamp_config.enabled);
}

#[test]
fn a_zero_target_size_is_refused_by_the_engine() {
    // `validate_project_structure` sıfır boyutu reddeder; arayüz sıfır
    // gönderirse kullanıcı "Ekler hazırlanamadı" görür ve sebebi yanlış olur.
    let stamp = r#"{"enabled":true,"position":"bottom_right","font_size":9.0,"margin_pt":18.0,"show_badge":true}"#;
    let project: Project = serde_json::from_str(&ui_payload(stamp, 0)).expect("çözülür");
    let err = ekler_core::persistence::validate_project_structure(&project)
        .expect_err("sıfır hedef boyut reddedilmeli");
    assert!(err.to_string().contains("Geçersiz boyut"), "{err}");
}

#[test]
fn the_default_project_passes_validation() {
    // Motorun kendi varsayılanı geçerlidir; arayüz de onu kullanmalı.
    let p = Project::default();
    assert_eq!(p.target_size_bytes, DEFAULT_TARGET_SIZE_BYTES);
    ekler_core::persistence::validate_project_structure(&p).expect("varsayılan proje geçerli olmalı");
}

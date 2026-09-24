//! WebView'in kendi tarayıcı kısayolları uygulamanın parçası değildir.
//!
//! Saha bulgusu (Windows): F5 ya da Ctrl+R bütün uygulamayı yeniden
//! yüklüyordu. Çalışma durumu (açık belgeler, Ekler grupları, karşılaştırma,
//! Düzenle'deki kaydedilmemiş sıra/döndürme) bellekte yaşadığı için yeniden
//! yükleme onu uyarısız siliyordu. Ctrl+S de belgeyi değil uygulamanın HTML
//! sayfasını "Farklı kaydet" ile diske yazıyordu. Uygulamada "Yenile" diye bir
//! komut yoktur; bunlar WebView2'nin tarayıcı özellikleridir ve Tauri 2.11 onları
//! kapatacak bir ayar sunmaz.
//!
//! Sağ tık menüsü sayfa tarafında kapanır (`src/shared-ui/browserSurface.ts`).
//! Klavye kısayolları orada kapatılamaz: WebView2 hızlandırıcıyı sayfa
//! JavaScript'inden ÖNCE işler (ICoreWebView2AcceleratorKeyPressedEventArgs2).
//! Bu yüzden Windows'ta yerel `AcceleratorKeyPressed` olayında, yalnız aşağıdaki
//! tuşlar için tarayıcının işlemesi kapatılır. Tuş sayfaya yine ulaşır — metin
//! yazmak ve AltGr bileşimleri etkilenmez.
//!
//! Ayar (`AreBrowserAcceleratorKeysEnabled`) yerine olay kullanılır: Tauri
//! pencereyi kullanıcı `setup`'ından ÖNCE kurar ve ayar ancak bir sonraki
//! gezinmede geçerli olur; ilk sayfa korumasız kalırdı. Ayrıca ayar Ctrl+F, yakınlaştırma gibi
//! zararsız kısayolları da toptan kapatırdı.

/// Windows sanal tuş kodları (winuser.h). Karar platformdan bağımsız kalsın
/// diye burada; macOS'ta da test edilir.
pub mod vk {
    pub const LEFT: u32 = 0x25;
    pub const RIGHT: u32 = 0x27;
    pub const R: u32 = 0x52;
    pub const S: u32 = 0x53;
    pub const F5: u32 = 0x74;
    pub const BROWSER_BACK: u32 = 0xA6;
    pub const BROWSER_FORWARD: u32 = 0xA7;
    pub const BROWSER_REFRESH: u32 = 0xA8;
}

/// Bir tuşa basış: tuş ve o anda basılı değiştiriciler.
#[derive(Debug, Clone, Copy, Default)]
pub struct Chord {
    pub key: u32,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// Tarayıcı bu tuşu KENDİSİ işlememeli mi?
///
/// * Yeniden yükleme: F5 (her değiştiriciyle), Ctrl+R, Ctrl+Shift+R, klavyenin
///   Yenile tuşu.
/// * Sayfayı kaydetme: Ctrl+S ("Farklı kaydet" HTML sayfasını yazar).
/// * Geçmişte gezinme: Alt+←/→ ve klavyenin Geri/İleri tuşları. Uygulama tek
///   sayfadır; tarayıcı geçmişi uygulamanın durumu değildir.
///
/// AltGr, Windows'ta Ctrl+Alt olarak gelir; AltGr+R/S bir karakter yazar,
/// dokunulmaz. Düzenleme kısayolları (Ctrl+C/V/X/A/Z) ve uygulamanın kendi
/// kısayolları (Ctrl+O, Ctrl+,) listede yoktur.
pub fn browser_must_not_handle(chord: Chord) -> bool {
    match chord.key {
        vk::F5 | vk::BROWSER_REFRESH | vk::BROWSER_BACK | vk::BROWSER_FORWARD => true,
        vk::R | vk::S => chord.ctrl && !chord.alt,
        vk::LEFT | vk::RIGHT => chord.alt && !chord.ctrl,
        _ => false,
    }
}

/// Ana pencerenin WebView2'sine hızlandırıcı süzgecini takar.
///
/// Başarısızlık uygulamayı durdurmaz: koruma kurulamazsa uygulama eski
/// davranışla çalışır ve sebep günlüğe düşer.
#[cfg(windows)]
pub fn install(window: &tauri::WebviewWindow) {
    use webview2_com::AcceleratorKeyPressedEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2AcceleratorKeyPressedEventArgs2, COREWEBVIEW2_KEY_EVENT_KIND,
        COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    };
    use windows::core::Interface;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT,
    };

    let installed = window.with_webview(|webview| {
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(|_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
            let mut key = 0u32;
            unsafe {
                args.KeyEventKind(&mut kind)?;
                args.VirtualKey(&mut key)?;
            }
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
            {
                return Ok(());
            }
            let down = |vk: VIRTUAL_KEY| unsafe { GetKeyState(i32::from(vk.0)) } < 0;
            let chord = Chord {
                key,
                ctrl: down(VK_CONTROL),
                shift: down(VK_SHIFT),
                alt: down(VK_MENU),
            };
            if browser_must_not_handle(chord) {
                // Tercih: tarayıcı işlemesin ama tuş sayfaya ulaşsın. Eski
                // çalışma ortamlarında bu arayüz yoksa olay tamamen tüketilir.
                match args.cast::<ICoreWebView2AcceleratorKeyPressedEventArgs2>() {
                    Ok(args2) => unsafe { args2.SetIsBrowserAcceleratorKeyEnabled(false)? },
                    Err(_) => unsafe { args.SetHandled(true)? },
                }
            }
            Ok(())
        }));
        let mut token = 0i64;
        if let Err(e) = unsafe {
            webview
                .controller()
                .add_AcceleratorKeyPressed(&handler, &mut token)
        } {
            eprintln!("GölgeDosya: tarayıcı kısayol süzgeci kurulamadı: {e}");
        }
    });
    if let Err(e) = installed {
        eprintln!("GölgeDosya: WebView2 denetleyicisine ulaşılamadı: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(key: u32, ctrl: bool, shift: bool, alt: bool) -> Chord {
        Chord {
            key,
            ctrl,
            shift,
            alt,
        }
    }

    #[test]
    fn every_reload_chord_is_taken_from_the_browser() {
        for (ctrl, shift, alt) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, false),
        ] {
            assert!(
                browser_must_not_handle(chord(vk::F5, ctrl, shift, alt)),
                "F5 ctrl={ctrl} shift={shift}"
            );
        }
        assert!(browser_must_not_handle(chord(vk::R, true, false, false)));
        assert!(browser_must_not_handle(chord(vk::R, true, true, false)));
        assert!(browser_must_not_handle(chord(
            vk::BROWSER_REFRESH,
            false,
            false,
            false
        )));
    }

    #[test]
    fn saving_the_page_and_history_navigation_are_taken_from_the_browser() {
        assert!(browser_must_not_handle(chord(vk::S, true, false, false)));
        assert!(browser_must_not_handle(chord(vk::LEFT, false, false, true)));
        assert!(browser_must_not_handle(chord(
            vk::RIGHT,
            false,
            false,
            true
        )));
        assert!(browser_must_not_handle(chord(
            vk::BROWSER_BACK,
            false,
            false,
            false
        )));
        assert!(browser_must_not_handle(chord(
            vk::BROWSER_FORWARD,
            false,
            false,
            false
        )));
    }

    #[test]
    fn editing_typing_and_the_apps_own_shortcuts_reach_the_page() {
        const O: u32 = 0x4F;
        const COMMA: u32 = 0xBC; // VK_OEM_COMMA
        const F: u32 = 0x46;
        for key in [0x43, 0x56, 0x58, 0x41, 0x5A, 0x59, O, COMMA, F] {
            assert!(
                !browser_must_not_handle(chord(key, true, false, false)),
                "Ctrl+{key:#x} sayfaya ulaşmalı"
            );
        }
        // Düz harf yazmak ve AltGr (Ctrl+Alt) bileşimleri.
        assert!(!browser_must_not_handle(chord(vk::R, false, false, false)));
        assert!(!browser_must_not_handle(chord(vk::R, false, true, false)));
        assert!(!browser_must_not_handle(chord(vk::R, true, false, true)));
        assert!(!browser_must_not_handle(chord(vk::S, true, false, true)));
        // Ctrl+← kelime atlar; tarayıcı geçmişiyle ilgisi yok.
        assert!(!browser_must_not_handle(chord(
            vk::LEFT,
            true,
            false,
            false
        )));
        assert!(!browser_must_not_handle(chord(
            vk::LEFT,
            false,
            false,
            false
        )));
    }
}

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const OLE_SIGNATURE: &[u8; 8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";
const DOCX_SIGNATURE: &[u8; 4] = b"PK\x03\x04";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempConversionDir(PathBuf);

impl TempConversionDir {
    fn create() -> Result<Self, &'static str> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "DOC_CONVERSION")?
            .as_nanos();
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tr.degisikis.desktop-doc-{}-{nonce}-{sequence}",
            std::process::id(),
        ));
        fs::create_dir(&path).map_err(|_| "DOC_CONVERSION")?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempConversionDir {
    fn drop(&mut self) {
        if let Err(_error) = fs::remove_dir_all(&self.0) {
            #[cfg(debug_assertions)]
            eprintln!("DOC geçici dizini temizlenemedi: {_error}");
        }
    }
}

fn password_error(stderr: &[u8]) -> bool {
    let message = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    [
        "doc_password",
        "password",
        "encrypted",
        "encryption",
        "protected",
        "parola",
        "şifre",
    ]
    .iter()
    .any(|term| message.contains(term))
}

fn read_converted_docx(path: &Path) -> Result<Vec<u8>, &'static str> {
    let bytes = fs::read(path).map_err(|_| "DOC_CONVERSION")?;
    if !bytes.starts_with(DOCX_SIGNATURE) {
        return Err("DOC_CONVERSION");
    }
    Ok(bytes)
}

pub fn convert(contents: &[u8]) -> Result<Vec<u8>, &'static str> {
    if !contents.starts_with(OLE_SIGNATURE) {
        return Err("DOC_INVALID");
    }
    platform::convert(contents)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn convert(contents: &[u8]) -> Result<Vec<u8>, &'static str> {
        let temporary = TempConversionDir::create()?;
        let source = temporary.path().join("source.doc");
        let converted = temporary.path().join("converted.docx");
        fs::write(&source, contents).map_err(|_| "DOC_CONVERSION")?;

        // Dış süreç sınırından geçer: zaman aşımı ve ağ reddi burada uygulanır.
        // Önceden ikisi de yoktu.
        let result = process_bridge::run(
            process_bridge::Spawn::new(std::path::Path::new("/usr/bin/textutil"))
                .args(["-convert", "docx", "-output"])
                .arg(&converted)
                .arg("--")
                .arg(&source)
                .timeout(std::time::Duration::from_secs(60))
                .network(process_bridge::NetworkPolicy::Deny),
        );

        if let Err(failure) = result {
            let stderr = failure.stderr();
            #[cfg(debug_assertions)]
            eprintln!(
                "textutil DOC dönüşümü başarısız: {}",
                String::from_utf8_lossy(stderr)
            );
            return Err(if password_error(stderr) {
                "DOC_PASSWORD"
            } else {
                "DOC_CONVERSION"
            });
        }

        read_converted_docx(&converted)
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::{ffi::OsString, os::windows::process::CommandExt, process::Command};

    const WORD_CONVERTER: &str = include_str!("word_to_docx.ps1");
    const WORD_PROVIDER_CHECK: &str =
        "if ([type]::GetTypeFromProgID('Word.Application')) { exit 0 } else { exit 20 }";
    /// CREATE_NO_WINDOW: uygulama GUI subsystem ile çalıştığı için konsolu yoktur.
    /// Bu bayrak olmadan Windows, konsol uygulaması olan powershell.exe için
    /// dönüşüm süresince görünür bir konsol penceresi tahsis eder.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn windowless_command(program: &Path) -> Command {
        let mut command = Command::new(program);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }

    fn powershell_executable() -> Result<PathBuf, &'static str> {
        let system_root = std::env::var_os("SystemRoot").ok_or("DOC_CONVERSION")?;
        let executable = PathBuf::from(system_root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        if executable.is_file() {
            Ok(executable)
        } else {
            Err("DOC_CONVERSION")
        }
    }

    fn word_provider_available(powershell: &Path) -> Result<bool, &'static str> {
        let status = windowless_command(powershell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                WORD_PROVIDER_CHECK,
            ])
            .status()
            .map_err(|_| "DOC_CONVERSION")?;
        Ok(status.success())
    }

    fn powershell_arguments(script: &Path, source: &Path, converted: &Path) -> Vec<OsString> {
        [
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-ExecutionPolicy"),
            OsString::from("Bypass"),
            OsString::from("-File"),
            script.as_os_str().to_owned(),
            source.as_os_str().to_owned(),
            converted.as_os_str().to_owned(),
        ]
        .into_iter()
        .collect()
    }

    fn conversion_error(status_code: Option<i32>, stderr: &[u8]) -> &'static str {
        match status_code {
            Some(20) => "DOC_WINDOWS_WORD_REQUIRED",
            Some(21) => "DOC_PASSWORD",
            Some(22) => "DOC_INVALID",
            _ if password_error(stderr) => "DOC_PASSWORD",
            _ => "DOC_CONVERSION",
        }
    }

    pub fn convert(contents: &[u8]) -> Result<Vec<u8>, &'static str> {
        let powershell = powershell_executable()?;
        if !word_provider_available(&powershell)? {
            return Err("DOC_WINDOWS_WORD_REQUIRED");
        }

        let temporary = TempConversionDir::create()?;
        let source = temporary.path().join("source.doc");
        let converted = temporary.path().join("converted.docx");
        let script = temporary.path().join("word-to-docx.ps1");
        fs::write(&source, contents).map_err(|_| "DOC_CONVERSION")?;
        fs::write(&script, WORD_CONVERTER).map_err(|_| "DOC_CONVERSION")?;

        let output = windowless_command(&powershell)
            .args(powershell_arguments(&script, &source, &converted))
            .output()
            .map_err(|_| "DOC_CONVERSION")?;

        if !output.status.success() {
            #[cfg(debug_assertions)]
            eprintln!(
                "Word DOC dönüşümü başarısız: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Err(conversion_error(output.status.code(), &output.stderr));
        }

        read_converted_docx(&converted)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn detects_word_provider_without_starting_word() {
            let powershell = powershell_executable().expect("Windows PowerShell");
            let detected = word_provider_available(&powershell).expect("provider detection");
            if !detected {
                assert_eq!(convert(OLE_SIGNATURE), Err("DOC_WINDOWS_WORD_REQUIRED"));
            }
        }

        #[test]
        fn paths_are_passed_as_distinct_arguments_without_script_interpolation() {
            let source = Path::new(r"C:\Users\Raci Çetin Yüksekbaş\A % & ' (1)\sözleşme.doc");
            let converted = Path::new(r"C:\Temp\çıktı dosyası.docx");
            let script = Path::new(r"C:\Temp\word-to-docx.ps1");
            let arguments = powershell_arguments(script, source, converted);

            assert_eq!(arguments[6], script.as_os_str());
            assert_eq!(arguments[7], source.as_os_str());
            assert_eq!(arguments[8], converted.as_os_str());
            assert!(!WORD_CONVERTER.contains(source.to_string_lossy().as_ref()));
        }

        #[test]
        fn maps_provider_password_and_invalid_document_failures() {
            assert_eq!(
                conversion_error(Some(20), b"DOC_WINDOWS_WORD_REQUIRED"),
                "DOC_WINDOWS_WORD_REQUIRED"
            );
            assert_eq!(conversion_error(Some(21), b"DOC_PASSWORD"), "DOC_PASSWORD");
            assert_eq!(conversion_error(Some(22), b"DOC_INVALID"), "DOC_INVALID");
        }

        #[test]
        fn failed_conversion_cleans_its_temporary_directory() {
            let before = temporary_directory_count();
            let _ = convert(OLE_SIGNATURE);
            assert_eq!(temporary_directory_count(), before);
        }

        fn temporary_directory_count() -> usize {
            fs::read_dir(std::env::temp_dir())
                .expect("temp list")
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("tr.degisikis.desktop-doc-")
                })
                .count()
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn convert(_contents: &[u8]) -> Result<Vec<u8>, &'static str> {
        Err("DOC_UNSUPPORTED_PLATFORM")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_doc_fails_closed() {
        assert_eq!(convert(b"not a compound document"), Err("DOC_INVALID"));
    }

    #[test]
    fn converted_output_must_have_docx_magic_bytes() {
        let temporary = TempConversionDir::create().expect("fixture temp");
        let converted = temporary.path().join("converted.docx");
        fs::write(&converted, b"not a zip").expect("fixture write");
        assert_eq!(read_converted_docx(&converted), Err("DOC_CONVERSION"));
        fs::write(&converted, b"PK\x03\x04fixture").expect("fixture write");
        assert_eq!(
            read_converted_docx(&converted),
            Ok(b"PK\x03\x04fixture".to_vec())
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn generated_binary_doc_converts_and_temporary_files_are_cleaned() {
        use std::process::Command;

        let fixture = TempConversionDir::create().expect("fixture temp");
        let source = fixture.path().join("fixture.rtf");
        let binary_doc = fixture.path().join("fixture.doc");
        fs::write(
            &source,
            r#"{\rtf1\ansi\deff0 {\fonttbl {\f0 Helvetica;}}\f0\fs24 Local DOC fixture.}"#,
        )
        .expect("fixture write");
        let status = Command::new("/usr/bin/textutil")
            .args(["-convert", "doc", "-output"])
            .arg(&binary_doc)
            .arg("--")
            .arg(&source)
            .status()
            .expect("textutil fixture");
        assert!(status.success());
        let contents = fs::read(&binary_doc).expect("fixture doc");
        assert!(contents.starts_with(OLE_SIGNATURE));

        let before = temporary_directory_count();
        let converted = convert(&contents).expect("DOC conversion");
        let after = temporary_directory_count();

        assert!(converted.starts_with(DOCX_SIGNATURE));
        assert_eq!(after, before);
    }

    #[cfg(target_os = "macos")]
    fn temporary_directory_count() -> usize {
        fs::read_dir(std::env::temp_dir())
            .expect("temp list")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("tr.degisikis.desktop-doc-")
            })
            .count()
    }
}

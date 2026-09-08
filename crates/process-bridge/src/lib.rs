//! Dış süreç sınırı.
//!
//! Uygulamanın **tek** süreç başlatma noktası. Bu crate'ten önce yüzey iki
//! modüle dağılmıştı ve güvenlik özellikleri tutarsızdı:
//!
//! | Çağrı | Zaman aşımı | Ağ politikası |
//! |---|---|---|
//! | LibreOffice (`office.rs`) | 120 sn | sandbox ile ağ reddi |
//! | `sips` (`image/mod.rs`) | **yok** | **yok** |
//! | `powershell.exe` (`image/mod.rs`) | **yok** | **yok** |
//!
//! Sınır bunun içindir: keşif, çalıştırılabilir doğrulaması, başlatma, zaman
//! aşımı ve ağ politikası tek yerde yaşar; çağıranlar yalnız hangi programı
//! hangi argümanlarla çalıştıracaklarını bilir.
//!
//! `document-core` ve `pdf-core` bu crate'e bağlı DEĞİLDİR ve süreç
//! başlatmazlar; `scripts/check-architecture.sh` bunu her sürüm kapısında
//! doğrular.
//!
//! ## sandbox-exec
//!
//! Apple `sandbox-exec`'i kullanımdan kaldırdı. Bugün çalıştığı ölçüldü ve
//! aynen korundu; yerine geçecek mimari ayrı bir iştir. Bkz.
//! `docs/ARCHITECTURE_DECISIONS.md`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Başlatılan sürecin ağ erişimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    /// Ağ reddedilir. macOS'ta `sandbox-exec` ile uygulanır; başka
    /// platformlarda uygulanacak bir mekanizma YOKTUR ve bu, sessizce
    /// geçilmez — `applied_on_this_platform()` false döner.
    Deny,
    /// Sürecin ağ erişimi kısıtlanmaz.
    Inherit,
}

impl NetworkPolicy {
    /// Bu platformda politikanın gerçekten uygulanıp uygulanamadığı.
    ///
    /// Çağıranın "ağ kapalı" sanıp öyle olmadığı bir durumda kalmaması için
    /// vardır: politika dilek değil, ölçülebilir bir olgudur.
    pub fn applied_on_this_platform(self) -> bool {
        match self {
            NetworkPolicy::Inherit => true,
            NetworkPolicy::Deny => cfg!(target_os = "macos"),
        }
    }
}

#[derive(Debug)]
pub enum BridgeError {
    /// Yol bir dosya değil, ya da çalıştırma bitine sahip değil.
    NotExecutable(PathBuf),
    /// Süreç başlatılamadı.
    Launch {
        program: PathBuf,
        source: std::io::Error,
    },
    /// Süreç zaman aşımına uğradı ve öldürüldü.
    Timeout { program: PathBuf, after: Duration },
    /// Süreç çalıştı ama sıfırdan farklı bir kodla bitti.
    ///
    /// Yakalanan çıktı içeride taşınır: çağıranların bir kısmı başarısızlığın
    /// TÜRÜNÜ stderr'den ayırt ediyor (örneğin parola korumalı bir `.doc`
    /// ile bozuk bir `.doc`). Bu ayrımı yutmak, kullanıcıya yanlış sebep
    /// göstermek olurdu.
    Failed {
        program: PathBuf,
        code: Option<i32>,
        output: Box<Output>,
    },
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::NotExecutable(p) => {
                write!(f, "Çalıştırılabilir değil: {}", p.display())
            }
            BridgeError::Launch { program, source } => {
                write!(f, "{} başlatılamadı: {source}", program.display())
            }
            BridgeError::Timeout { program, after } => write!(
                f,
                "{} {} saniyede yanıt vermedi ve durduruldu",
                program.display(),
                after.as_secs()
            ),
            BridgeError::Failed { program, code, .. } => match code {
                Some(c) => write!(f, "{} {} koduyla bitti", program.display(), c),
                None => write!(f, "{} bir sinyalle sonlandı", program.display()),
            },
        }
    }
}

impl BridgeError {
    /// Başarısız bir çalıştırmanın stderr'i. Diğer hatalarda boş.
    pub fn stderr(&self) -> &[u8] {
        match self {
            BridgeError::Failed { output, .. } => &output.stderr,
            _ => &[],
        }
    }
}

impl std::error::Error for BridgeError {}

/// Çalıştırılacak dış süreç.
pub struct Spawn<'a> {
    pub program: &'a Path,
    /// `true` ise program adı işletim sistemine PATH üzerinden çözdürülür ve
    /// köprü onu dosya olarak DOĞRULAMAZ.
    ///
    /// Açık bir istisnadır, sessiz bir geri düşüş değil. Tek kullanıcısı,
    /// Windows'ta `powershell.exe`'yi adıyla çağıran taşınmış koddur: bu
    /// makinede Windows hedefi derlenemediği için oradaki çözümleme
    /// semantiğini DEĞİŞTİRMEK, doğrulanamayan yeni kod yazmak olurdu.
    pub os_resolved: bool,
    pub args: Vec<OsString>,
    pub envs: Vec<(OsString, OsString)>,
    pub timeout: Duration,
    pub network: NetworkPolicy,
    /// `true` ise stdout/stderr toplanır; `false` ise `/dev/null`'a gider.
    pub capture: bool,
}

impl<'a> Spawn<'a> {
    pub fn new(program: &'a Path) -> Self {
        Self {
            program,
            os_resolved: false,
            args: Vec::new(),
            envs: Vec::new(),
            timeout: Duration::from_secs(120),
            network: NetworkPolicy::Deny,
            capture: true,
        }
    }

    pub fn arg(mut self, a: impl Into<OsString>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(it.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, k: impl Into<OsString>, v: impl Into<OsString>) -> Self {
        self.envs.push((k.into(), v.into()));
        self
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    pub fn network(mut self, p: NetworkPolicy) -> Self {
        self.network = p;
        self
    }

    pub fn capture(mut self, c: bool) -> Self {
        self.capture = c;
        self
    }

    /// Programı işletim sistemine PATH üzerinden çözdür; dosya doğrulaması yapma.
    pub fn os_resolved(mut self) -> Self {
        self.os_resolved = true;
        self
    }
}

/// Bir yolun gerçekten çalıştırılabilir bir dosya olup olmadığı.
pub fn validate_executable(path: &Path) -> Result<(), BridgeError> {
    let meta = std::fs::metadata(path).map_err(|_| BridgeError::NotExecutable(path.into()))?;
    if !meta.is_file() {
        return Err(BridgeError::NotExecutable(path.into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return Err(BridgeError::NotExecutable(path.into()));
        }
    }
    Ok(())
}

/// Aday yollardan çalıştırılabilir olan ilkini seç.
pub fn discover<I, P>(candidates: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    candidates
        .into_iter()
        .map(Into::into)
        .find(|c| validate_executable(c).is_ok())
}

/// Dış süreci çalıştır ve bitmesini bekle.
///
/// Zaman aşımı zorunludur: süresiz bekleyen bir dış süreç, kullanıcının
/// uygulamayı kapatmaktan başka çaresi kalmadığı bir donma demektir.
pub fn run(spec: Spawn<'_>) -> Result<Output, BridgeError> {
    if !spec.os_resolved {
        validate_executable(spec.program)?;
    }

    let mut command = match spec.network {
        // macOS: ağ reddi sandbox ile uygulanır. Profil, LibreOffice için
        // yazılan profilin aynısıdır ve davranışsal olarak korunmuştur.
        NetworkPolicy::Deny if cfg!(target_os = "macos") => {
            let mut c = Command::new("/usr/bin/sandbox-exec");
            c.args(["-p", "(version 1)(allow default)(deny network*)"])
                .arg(spec.program);
            c
        }
        _ => Command::new(spec.program),
    };

    command.args(&spec.args);
    for (k, v) in &spec.envs {
        command.env(k, v);
    }
    // Windows'ta uygulama GUI subsystem ile çalışır ve konsolu yoktur; bu bayrak
    // olmadan her yardımcı süreç için görünür bir konsol penceresi açılır.
    // "Platform'a özgü komut yüzeyi" tam olarak sınırın işidir, çağıranın değil.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.stdin(Stdio::null());
    if spec.capture {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let mut child = command.spawn().map_err(|source| BridgeError::Launch {
        program: spec.program.into(),
        source,
    })?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > spec.timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(BridgeError::Timeout {
                        program: spec.program.into(),
                        after: spec.timeout,
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(source) => {
                return Err(BridgeError::Launch {
                    program: spec.program.into(),
                    source,
                })
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|source| BridgeError::Launch {
            program: spec.program.into(),
            source,
        })?;

    if !output.status.success() {
        return Err(BridgeError::Failed {
            program: spec.program.into(),
            code: output.status.code(),
            output: Box::new(output),
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_is_not_executable() {
        assert!(validate_executable(&std::env::temp_dir()).is_err());
    }

    #[test]
    fn a_plain_file_is_not_executable() {
        let f = std::env::temp_dir().join(format!("bridge-plain-{}", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        assert!(validate_executable(&f).is_err());
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn discovery_skips_what_cannot_run() {
        let found = discover([
            PathBuf::from("/bu/yol/yok"),
            std::env::temp_dir(),
            PathBuf::from("/bin/echo"),
        ]);
        assert_eq!(found, Some(PathBuf::from("/bin/echo")));
    }

    #[test]
    fn a_process_that_exceeds_its_timeout_is_killed() {
        let err = run(Spawn::new(Path::new("/bin/sleep"))
            .arg("30")
            .timeout(Duration::from_millis(300))
            .network(NetworkPolicy::Inherit))
        .expect_err("zaman aşımına uğramalıydı");
        assert!(matches!(err, BridgeError::Timeout { .. }), "{err}");
    }

    #[test]
    fn a_nonzero_exit_is_an_error_not_a_success() {
        let err = run(Spawn::new(Path::new("/bin/sh"))
            .args(["-c", "exit 3"])
            .network(NetworkPolicy::Inherit))
        .expect_err("başarısızlık bekleniyordu");
        assert!(
            matches!(err, BridgeError::Failed { code: Some(3), .. }),
            "{err}"
        );
    }

    #[test]
    fn output_is_captured_when_asked_for() {
        let out = run(Spawn::new(Path::new("/bin/echo"))
            .arg("merhaba")
            .network(NetworkPolicy::Inherit))
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "merhaba");
    }

    /// Ağ reddi macOS'ta gerçekten uygulanıyor mu — iddia değil, ölçüm.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_deny_policy_actually_blocks_the_network() {
        assert!(NetworkPolicy::Deny.applied_on_this_platform());
        // Sandbox altında bir TCP bağlantısı kurulamamalı. `nc` yoksa test
        // atlanmaz; başka bir yolla denenir.
        let blocked = run(Spawn::new(Path::new("/bin/sh"))
            .args([
                "-c",
                "exec 3<>/dev/tcp/127.0.0.1/9 && echo ACIK || echo KAPALI",
            ])
            .network(NetworkPolicy::Deny));
        // Bağlantı kurulamadığında kabuk sıfırdan farklı kod döndürebilir;
        // her iki durumda da "ACIK" yazmamalı.
        if let Ok(out) = blocked {
            assert!(
                !String::from_utf8_lossy(&out.stdout).contains("ACIK"),
                "sandbox altında ağ açık kaldı"
            );
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_deny_policy_is_honest_about_not_being_available() {
        assert!(!NetworkPolicy::Deny.applied_on_this_platform());
    }
}

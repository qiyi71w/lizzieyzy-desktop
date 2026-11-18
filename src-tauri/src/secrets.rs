//! OS credential storage. Errors are deliberately constant and never retain subprocess output.
//! The program creates one [`Store`] at start and hands it to every credential owner; tests hand
//! them an in-process [`Store::memory`] instead, so they never reach the machine's storage.
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Store {
    application: String,
    directory: PathBuf,
    backend: Backend,
}
#[derive(Clone)]
enum Backend {
    /// The platform storage, probed once when the store is created.
    Platform { available: bool },
    /// In-process entries by (application, kind, account); `None` is a machine without storage.
    #[cfg(test)]
    Memory(Option<Vault>),
}
/// The entries of a [`Store::memory`], shared with the test that inspects them.
#[cfg(test)]
pub type Vault = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<(String, String, String), String>>>;
fn unavailable() -> io::Error {
    io::Error::other("System secure credential storage is unavailable.")
}
fn normalize(account: &str) -> String {
    let account = account.trim().to_lowercase();
    if account.is_empty() {
        "default".into()
    } else {
        account
    }
}
impl Store {
    pub fn new(application: String, config: &Path) -> Self {
        Self {
            application,
            directory: config.join("secure-credentials"),
            backend: Backend::Platform {
                available: platform::available(),
            },
        }
    }
    /// A test store for `application` holding `vault`'s entries; `None` has no storage at all.
    #[cfg(test)]
    pub fn memory(application: &str, vault: Option<Vault>) -> Self {
        Self {
            application: application.into(),
            directory: PathBuf::new(),
            backend: Backend::Memory(vault),
        }
    }
    pub fn available(&self) -> bool {
        match &self.backend {
            Backend::Platform { available } => *available,
            #[cfg(test)]
            Backend::Memory(vault) => vault.is_some(),
        }
    }
    pub fn backend(&self) -> &'static str {
        platform::BACKEND
    }
    fn get(&self, application: &str, directory: &Path, kind: &str, account: &str) -> io::Result<Option<String>> {
        match &self.backend {
            Backend::Platform { .. } => platform::read(application, directory, kind, account),
            #[cfg(test)]
            Backend::Memory(vault) => {
                let vault = vault.as_ref().ok_or_else(unavailable)?.lock().unwrap();
                Ok(vault.get(&(application.into(), kind.into(), account.into())).cloned())
            }
        }
    }
    fn put(&self, kind: &str, account: &str, secret: &str) -> io::Result<()> {
        match &self.backend {
            Backend::Platform { .. } => platform::write(&self.application, &self.directory, kind, account, secret),
            #[cfg(test)]
            Backend::Memory(vault) => {
                let mut vault = vault.as_ref().ok_or_else(unavailable)?.lock().unwrap();
                vault.insert((self.application.clone(), kind.into(), account.into()), secret.into());
                Ok(())
            }
        }
    }
    fn clear(&self, kind: &str, account: &str) -> io::Result<()> {
        match &self.backend {
            Backend::Platform { .. } => platform::delete(&self.application, &self.directory, kind, account),
            #[cfg(test)]
            Backend::Memory(vault) => {
                let mut vault = vault.as_ref().ok_or_else(unavailable)?.lock().unwrap();
                vault.remove(&(self.application.clone(), kind.into(), account.into()));
                Ok(())
            }
        }
    }
    pub fn read(&self, kind: &str, account: &str) -> io::Result<Option<String>> {
        if !self.available() {
            return Ok(None);
        }
        self.get(&self.application, &self.directory, kind, &normalize(account))
    }
    pub fn write(&self, kind: &str, account: &str, secret: &str) -> io::Result<()> {
        if !self.available() || secret.is_empty() {
            return Err(unavailable());
        }
        self.put(kind, &normalize(account), secret)?;
        if self.read(kind, account)?.as_deref() != Some(secret) {
            return Err(unavailable());
        }
        Ok(())
    }
    pub fn delete(&self, kind: &str, account: &str) -> io::Result<()> {
        if !self.available() {
            return Ok(());
        }
        self.clear(kind, &normalize(account))
    }
    /// The only Java identity access: read-only, called by explicit Java import.
    pub fn read_java_api_key(&self, source: &Path, account: &str) -> io::Result<Option<String>> {
        if !self.available() {
            return Ok(None);
        }
        self.get(
            "lizzieyzy-next",
            &source.join("secure-credentials"),
            "api-key",
            &normalize(account),
        )
    }
    /// Explicit cloud credential import; the Java store remains read-only.
    pub fn read_java_cloud_secret(&self, source: &Path, account: &str, password: bool) -> io::Result<Option<String>> {
        if !self.available() {
            return Ok(None);
        }
        self.get(
            "lizzieyzy-next",
            &source.join("secure-credentials"),
            if password { "password" } else { "account-token" },
            &normalize(account),
        )
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    pub const BACKEND: &str = "linux-secret-service";
    fn run(arguments: &[&str], secret: Option<&str>) -> io::Result<(i32, Vec<u8>)> {
        let mut child = Command::new("secret-tool")
            .args(arguments)
            .stdin(if secret.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| unavailable())?;
        let stdout = child.stdout.take().ok_or_else(unavailable)?;
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.take(1_048_577).read_to_end(&mut bytes).map(|_| bytes)
        });
        let writer = secret.map(|secret| {
            let mut input = child.stdin.take();
            let mut bytes = secret.as_bytes().to_vec();
            std::thread::spawn(move || {
                let result = input
                    .as_mut()
                    .ok_or_else(unavailable)
                    .and_then(|input| input.write_all(&bytes));
                bytes.fill(0);
                result
            })
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(unavailable());
                }
            }
        };
        let output = reader.join().map_err(|_| unavailable())?.map_err(|_| unavailable())?;
        if let Some(writer) = writer {
            writer.join().map_err(|_| unavailable())?.map_err(|_| unavailable())?;
        }
        if output.len() > 1_048_576 {
            return Err(unavailable());
        }
        Ok((status?.code().unwrap_or(-1), output))
    }
    pub fn available() -> bool {
        run(&["--version"], None).is_ok_and(|(code, _)| code == 0)
    }
    pub fn read(app: &str, _: &Path, kind: &str, account: &str) -> io::Result<Option<String>> {
        let (code, mut bytes) = run(&["lookup", "application", app, "kind", kind, "account", account], None)?;
        // Only status 1 means absent; a locked or failing vault is not a revoked login.
        if code != 0 {
            bytes.fill(0);
            return if code == 1 { Ok(None) } else { Err(unavailable()) };
        }
        while bytes.last().is_some_and(|b| matches!(b, b'\r' | b'\n')) {
            bytes.pop();
        }
        let value = String::from_utf8(bytes).map_err(|_| unavailable())?;
        Ok((!value.is_empty()).then_some(value))
    }
    pub fn write(app: &str, _: &Path, kind: &str, account: &str, secret: &str) -> io::Result<()> {
        let (code, _) = run(
            &[
                "store",
                "--label=LizzieYzy Tauri AI commentary",
                "application",
                app,
                "kind",
                kind,
                "account",
                account,
            ],
            Some(secret),
        )?;
        if code == 0 {
            Ok(())
        } else {
            Err(unavailable())
        }
    }
    pub fn delete(app: &str, _: &Path, kind: &str, account: &str) -> io::Result<()> {
        let (code, _) = run(&["clear", "application", app, "kind", kind, "account", account], None)?;
        if code == 0 || code == 1 {
            Ok(())
        } else {
            Err(unavailable())
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use security_framework::os::macos::keychain::SecKeychain;
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};
    static INTERACTION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    pub const BACKEND: &str = "macos-keychain";
    fn service(app: &str, kind: &str) -> String {
        format!(
            "{}.ai-commentary.{kind}",
            if app == "lizzieyzy-next" {
                "cn.lizzieyzy.next"
            } else {
                app
            }
        )
    }
    pub fn available() -> bool {
        true
    }
    pub fn read(app: &str, _: &Path, kind: &str, account: &str) -> io::Result<Option<String>> {
        let _serial = INTERACTION.lock().map_err(|_| unavailable())?;
        let _interaction =
            if kind == "chatgpt-session" && SecKeychain::user_interaction_allowed().map_err(|_| unavailable())? {
                Some(SecKeychain::disable_user_interaction().map_err(|_| unavailable())?)
            } else {
                None
            };
        match get_generic_password(&service(app, kind), account) {
            Ok(bytes) => {
                let value = String::from_utf8(bytes).map_err(|_| unavailable())?;
                Ok((!value.is_empty()).then_some(value))
            }
            Err(error) if error.code() == -25300 => Ok(None),
            Err(_) => Err(unavailable()),
        }
    }
    pub fn write(app: &str, _: &Path, kind: &str, account: &str, secret: &str) -> io::Result<()> {
        let _serial = INTERACTION.lock().map_err(|_| unavailable())?;
        set_generic_password(&service(app, kind), account, secret.as_bytes()).map_err(|_| unavailable())
    }
    pub fn delete(app: &str, _: &Path, kind: &str, account: &str) -> io::Result<()> {
        let _serial = INTERACTION.lock().map_err(|_| unavailable())?;
        match delete_generic_password(&service(app, kind), account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(_) => Err(unavailable()),
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use sha2::{Digest, Sha256};
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    pub const BACKEND: &str = "windows-dpapi";
    fn crypt(bytes: &[u8], protect: bool) -> io::Result<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(bytes.len()).map_err(|_| unavailable())?,
            pbData: bytes.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe {
            let result = if protect {
                CryptProtectData(
                    &input,
                    windows::core::PCWSTR::null(),
                    None,
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output)
            };
            result.map_err(|_| unavailable())?;
            let buffer = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
            let value = buffer.to_vec();
            buffer.fill(0);
            let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
            Ok(value)
        }
    }
    pub fn available() -> bool {
        crypt(b"lizzieyzy-tauri-dpapi-probe", true)
            .and_then(|v| crypt(&v, false))
            .is_ok_and(|v| v == b"lizzieyzy-tauri-dpapi-probe")
    }
    fn path(dir: &Path, kind: &str, account: &str) -> PathBuf {
        let hash = Sha256::digest(account.as_bytes());
        let hex: String = hash[..12].iter().map(|v| format!("{v:02x}")).collect();
        dir.join(format!("{kind}-{hex}.dpapi"))
    }
    pub fn read(_: &str, dir: &Path, kind: &str, account: &str) -> io::Result<Option<String>> {
        let file = path(dir, kind, account);
        if !file.is_file() {
            return Ok(None);
        }
        let encoded = std::fs::read_to_string(file).map_err(|_| unavailable())?;
        let cipher = STANDARD.decode(encoded.trim()).map_err(|_| unavailable())?;
        let mut bytes = crypt(&cipher, false)?;
        let value = String::from_utf8(bytes.clone()).map_err(|_| unavailable());
        bytes.fill(0);
        Ok(value.map(|v| (!v.is_empty()).then_some(v))?)
    }
    pub fn write(_: &str, dir: &Path, kind: &str, account: &str, secret: &str) -> io::Result<()> {
        std::fs::create_dir_all(dir).map_err(|_| unavailable())?;
        let cipher = crypt(secret.as_bytes(), true)?;
        let mut check = crypt(&cipher, false)?;
        let matches = check == secret.as_bytes();
        check.fill(0);
        if !matches {
            return Err(unavailable());
        }
        let file = path(dir, kind, account);
        let temporary = file.with_extension("dpapi.tmp");
        std::fs::write(&temporary, STANDARD.encode(cipher)).map_err(|_| unavailable())?;
        if std::fs::rename(&temporary, &file).is_err() {
            std::fs::copy(&temporary, &file).map_err(|_| unavailable())?;
            std::fs::remove_file(&temporary).map_err(|_| unavailable())?;
        }
        Ok(())
    }
    pub fn delete(_: &str, dir: &Path, kind: &str, account: &str) -> io::Result<()> {
        match std::fs::remove_file(path(dir, kind, account)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(unavailable()),
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    use super::*;
    pub const BACKEND: &str = "unavailable";
    pub fn available() -> bool {
        false
    }
    pub fn read(_: &str, _: &Path, _: &str, _: &str) -> io::Result<Option<String>> {
        Ok(None)
    }
    pub fn write(_: &str, _: &Path, _: &str, _: &str, _: &str) -> io::Result<()> {
        Err(unavailable())
    }
    pub fn delete(_: &str, _: &Path, _: &str, _: &str) -> io::Result<()> {
        Ok(())
    }
}

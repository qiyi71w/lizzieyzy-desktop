//! Java `downloadRemoteWeight` / `downloadAsset`: resume, integrity, cancellation and progress.
use super::{check_cancel, invalid, validate_sha256, ResourceError, ResourceResult};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

/// `total_length` is the entire resource length, not the length of a Range response body.
pub struct FetchResponse {
    pub status: u16,
    pub total_length: Option<u64>,
    pub body: Box<dyn Read + Send>,
}
/// Adapter must honour the caller's proxy configuration and bound connection/read timeouts.
/// Range responses must start at the requested offset (validate Content-Range in the adapter).
pub trait Fetch {
    fn get(&self, url: &str, start: Option<u64>) -> ResourceResult<FetchResponse>;
}
#[derive(Clone, Debug)]
pub struct DownloadSpec {
    pub url: String,
    pub file_name: String,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub bytes: u64,
    pub total: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Downloaded {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub reused: bool,
}

pub fn sha256_file(path: &Path) -> ResourceResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
/// A missing size means Java's remote-network rule: strictly greater than one MiB.
pub fn verify_file(path: &Path, size: Option<u64>, sha: Option<&str>) -> ResourceResult<String> {
    let meta = fs::metadata(path)?;
    if !meta.is_file() {
        return Err(invalid("Resource is not a regular file"));
    }
    if let Some(size) = size {
        if meta.len() != size {
            return Err(ResourceError::Integrity {
                expected: format!("{size} bytes"),
                actual: format!("{} bytes", meta.len()),
            });
        }
    } else if meta.len() <= 1024 * 1024 {
        return Err(ResourceError::Integrity {
            expected: "> 1048576 bytes".into(),
            actual: format!("{} bytes", meta.len()),
        });
    }
    let actual = sha256_file(path)?;
    if let Some(expected) = sha {
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(ResourceError::Integrity {
                expected: expected.into(),
                actual,
            });
        }
    }
    Ok(actual)
}
fn valid_existing(path: &Path, spec: &DownloadSpec) -> Option<Downloaded> {
    if !path.is_file() {
        return None;
    }
    verify_file(path, spec.size, spec.sha256.as_deref())
        .ok()
        .and_then(|sha256| {
            fs::metadata(path).ok().map(|m| Downloaded {
                path: path.into(),
                sha256,
                size: m.len(),
                reused: true,
            })
        })
}
/// Never removes an old target before its replacement is fully downloaded and verified.
/// Cancellation retains `.part`; integrity failures discard it. Other I/O errors retain it.
pub fn download(
    fetch: &dyn Fetch,
    directory: &Path,
    spec: &DownloadSpec,
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> ResourceResult<Downloaded> {
    if !super::matches_pattern(r"^[A-Za-z0-9][A-Za-z0-9._-]*$", &spec.file_name) || spec.file_name.ends_with(".part") {
        return Err(invalid("Unsafe download file name"));
    }
    if let Some(sha) = &spec.sha256 {
        validate_sha256(sha)?;
    }
    check_cancel(cancel)?;
    fs::create_dir_all(directory)?;
    let target = directory.join(&spec.file_name);
    let part = directory.join(format!("{}.part", spec.file_name));
    if let Some(existing) = valid_existing(&target, spec) {
        progress(DownloadProgress {
            bytes: existing.size,
            total: Some(existing.size),
        });
        return Ok(existing);
    }
    // Do not treat an unknown-size unhashed partial as complete: it needs a server response.
    if spec.size.is_some() || spec.sha256.is_some() {
        if let Some(mut complete) = valid_existing(&part, spec) {
            check_cancel(cancel)?;
            replace_file(&part, &target)?;
            complete.path = target;
            progress(DownloadProgress {
                bytes: complete.size,
                total: Some(complete.size),
            });
            return Ok(complete);
        }
    }
    if part.is_file()
        && spec
            .size
            .is_some_and(|n| fs::metadata(&part).is_ok_and(|m| m.len() > n))
    {
        fs::remove_file(&part)?;
    }
    let mut offset = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let mut restarted = false;
    loop {
        check_cancel(cancel)?;
        let mut response = fetch.get(&spec.url, (offset > 0).then_some(offset))?;
        if offset > 0 && (response.status == 416 || response.status == 200) {
            if response.status == 416 {
                if let Some(mut complete) = valid_existing(&part, spec) {
                    check_cancel(cancel)?;
                    replace_file(&part, &target)?;
                    complete.path = target;
                    progress(DownloadProgress {
                        bytes: complete.size,
                        total: Some(complete.size),
                    });
                    return Ok(complete);
                }
            }
            fs::remove_file(&part)?;
            offset = 0;
            if restarted {
                return Err(invalid("Server rejected a restarted download"));
            }
            restarted = true;
            continue;
        }
        if !(200..300).contains(&response.status) {
            return Err(ResourceError::Http(response.status));
        }
        let resumed = offset > 0 && response.status == 206;
        if !resumed {
            offset = 0;
        }
        let total = spec.size.or(response.total_length);
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&part)?;
        let mut bytes = offset;
        let mut buffer = [0; 64 * 1024];
        let mut last = None;
        loop {
            check_cancel(cancel)?;
            let n = response.body.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            check_cancel(cancel)?;
            output.write_all(&buffer[..n])?;
            bytes += n as u64;
            if last.is_none_or(|t: Instant| t.elapsed() >= Duration::from_millis(120)) || total == Some(bytes) {
                progress(DownloadProgress { bytes, total });
                last = Some(Instant::now());
            }
            check_cancel(cancel)?;
        }
        output.sync_all()?;
        drop(output);
        check_cancel(cancel)?;
        let digest = match verify_file(&part, spec.size.or(response.total_length), spec.sha256.as_deref()) {
            Ok(digest) => digest,
            Err(e @ ResourceError::Integrity { .. }) => {
                fs::remove_file(&part)?;
                return Err(e);
            }
            Err(e) => return Err(e),
        };
        // Unknown-size remote weights must still meet the Java >1MiB floor.
        if spec.size.is_none() && spec.sha256.is_none() && bytes <= 1024 * 1024 {
            fs::remove_file(&part)?;
            return Err(ResourceError::Integrity {
                expected: "> 1048576 bytes".into(),
                actual: format!("{bytes} bytes"),
            });
        }
        check_cancel(cancel)?;
        replace_file(&part, &target)?;
        progress(DownloadProgress {
            bytes,
            total: Some(bytes),
        });
        return Ok(Downloaded {
            path: target,
            sha256: digest,
            size: bytes,
            reused: false,
        });
    }
}
/// Windows rename does not replace an existing file. The backup is restored if replacement fails.
pub(crate) fn replace_file(staged: &Path, target: &Path) -> ResourceResult<()> {
    if !target.exists() {
        fs::rename(staged, target)?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        fs::rename(staged, target)?;
    }
    #[cfg(windows)]
    {
        let backup = super::store::sibling_path(target, "backup");
        fs::rename(target, &backup)?;
        if let Err(error) = fs::rename(staged, target) {
            fs::rename(&backup, target)?;
            return Err(error.into());
        }
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

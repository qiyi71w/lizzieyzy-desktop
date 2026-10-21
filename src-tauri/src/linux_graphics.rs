//! VMware's X11 DMA-BUF fence path can stall WebKit's compositor. Keep GL rendering,
//! but transfer its finished frames through shared memory on this driver only.

use std::ffi::{OsStr, OsString};
use std::path::Path;

const SHM: &str = "WEBKIT_DMABUF_RENDERER_FORCE_SHM";

/// Called only by main, before GTK, Tauri or any application threads are started.
/// Do not move this into Tauri setup: WebKit reads its transport choice before setup.
pub fn prepare() {
    if needs_shared_memory(|key| std::env::var_os(key), Path::new("/sys/class/drm")) {
        // At this single-threaded entrypoint no other thread can read the environment.
        std::env::set_var(SHM, "1");
        eprintln!("X11 vmwgfx: WebKit shared-memory frame transport; GL rendering retained");
    }
}

fn needs_shared_memory(env: impl Fn(&str) -> Option<OsString>, drm: &Path) -> bool {
    if env("XDG_SESSION_TYPE").as_deref() != Some(OsStr::new("x11"))
        || env("WAYLAND_DISPLAY").is_some()
        || env("DISPLAY").is_none_or(|display| display.is_empty())
    {
        return false;
    }
    // Explicit operator choices win, including values such as FORCE_SHM=0. Do not
    // infer the renderer from a VMware device when another GPU/driver was selected.
    for key in [
        SHM,
        "GDK_BACKEND",
        "LIBGL_ALWAYS_SOFTWARE",
        "LIBGL_ALWAYS_INDIRECT",
        "MESA_LOADER_DRIVER_OVERRIDE",
        "GALLIUM_DRIVER",
        "DRI_PRIME",
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
        "WEBKIT_SKIA_ENABLE_CPU_RENDERING",
    ] {
        if env(key).is_some() {
            return false;
        }
    }
    let Ok(entries) = std::fs::read_dir(drm) else { return false };
    let mut found_card = false;
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(number) = name.strip_prefix("card") else { continue };
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(driver) = std::fs::read_link(entry.path().join("device/driver")) else { return false };
        if driver.file_name() != Some(OsStr::new("vmwgfx")) {
            return false;
        }
        found_card = true;
    }
    found_card
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Drm(std::path::PathBuf);
    impl Drm {
        fn new(drivers: &[&str]) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "lizzie-graphics-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            for (index, driver) in drivers.iter().enumerate() {
                let device = path.join(format!("card{index}/device"));
                std::fs::create_dir_all(&device).unwrap();
                symlink(Path::new("/sys/bus/pci/drivers").join(driver), device.join("driver")).unwrap();
            }
            // Connector entries must not be mistaken for extra graphics cards.
            std::fs::create_dir(path.join("card0-Virtual-1")).unwrap();
            Self(path)
        }
    }
    impl Drop for Drm {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn decision(drm: &Drm, changes: &[(&str, &str)]) -> bool {
        needs_shared_memory(|key| {
            changes.iter().find(|(k, _)| *k == key).map(|(_, v)| OsString::from(v))
                .or_else(|| match key {
                    "XDG_SESSION_TYPE" => Some("x11".into()),
                    "DISPLAY" => Some(":0".into()),
                    _ => None,
                })
        }, &drm.0)
    }

    #[test]
    fn selects_only_unambiguous_vmwgfx_x11() {
        assert!(decision(&Drm::new(&["vmwgfx"]), &[]));
        for drivers in [&[][..], &["i915"][..], &["vmwgfx", "amdgpu"][..]] {
            assert!(!decision(&Drm::new(drivers), &[]));
        }
        let drm = Drm::new(&["vmwgfx"]);
        for change in [("XDG_SESSION_TYPE", "wayland"), ("XDG_SESSION_TYPE", ""),
            ("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", "")] {
            assert!(!decision(&drm, &[change]), "{change:?}");
        }
        std::fs::remove_file(drm.0.join("card0/device/driver")).unwrap();
        assert!(!decision(&drm, &[]));
    }

    #[test]
    fn preserves_explicit_graphics_choices() {
        let drm = Drm::new(&["vmwgfx"]);
        for choice in [(SHM, "0"), (SHM, "1"), ("GDK_BACKEND", "wayland"),
            ("LIBGL_ALWAYS_SOFTWARE", "1"), ("LIBGL_ALWAYS_INDIRECT", "1"),
            ("MESA_LOADER_DRIVER_OVERRIDE", "zink"), ("GALLIUM_DRIVER", "llvmpipe"),
            ("DRI_PRIME", "1"), ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
            ("WEBKIT_DISABLE_COMPOSITING_MODE", "1"), ("WEBKIT_SKIA_ENABLE_CPU_RENDERING", "1")] {
            assert!(!decision(&drm, &[choice]), "{choice:?}");
        }
    }
}

//! File-permission faults that hold whatever privileges the test process has.

/// Runs `f` on a thread that obeys file permission bits even when the process may bypass them
/// (root, or `CAP_DAC_OVERRIDE` in a user namespace): on Linux that thread drops those
/// capabilities from its own effective set, so a folder of mode `0555` really refuses writes.
/// Elsewhere the process's own rights apply.
pub fn obeying_permissions<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| {
        s.spawn(|| {
            #[cfg(target_os = "linux")]
            drop_permission_bypass();
            f()
        })
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

#[cfg(target_os = "linux")]
fn drop_permission_bypass() {
    #[repr(C)]
    struct Header {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Data {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    unsafe extern "C" {
        fn capget(header: *mut Header, data: *mut Data) -> i32;
        fn capset(header: *mut Header, data: *const Data) -> i32;
    }
    const VERSION_3: u32 = 0x2008_0522;
    // CAP_DAC_OVERRIDE, CAP_DAC_READ_SEARCH and CAP_FOWNER.
    const BYPASS: u32 = (1 << 1) | (1 << 2) | (1 << 3);
    let mut header = Header { version: VERSION_3, pid: 0 };
    let mut data = [Data::default(); 2];
    // SAFETY: version 3 takes two data structs; pid 0 is the calling thread.
    unsafe {
        assert_eq!(capget(&mut header, data.as_mut_ptr()), 0, "capget: {}", std::io::Error::last_os_error());
        data[0].effective &= !BYPASS;
        assert_eq!(capset(&mut header, data.as_ptr()), 0, "capset: {}", std::io::Error::last_os_error());
    }
}

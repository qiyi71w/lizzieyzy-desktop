//! A picture of one area of the screen in physical pixels, for 抓取死活题 (Java `Robot`
//! `createScreenCapture`). Windows reads the desktop with GDI, macOS with Core Graphics, an X11
//! session with `GetImage` on the root window, and a Wayland session through the desktop portal's
//! screenshot (which may ask the user for permission once).

/// RGBA pixels, row-major.
pub struct Screenshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The area `x, y, width, height` in physical pixels of the virtual screen; `scale` is the
/// display's scale factor (macOS works in points).
pub fn capture(x: i32, y: i32, width: u32, height: u32, scale: f64) -> Result<Screenshot, String> {
    if width == 0 || height == 0 {
        return Err("empty screen area".into());
    }
    platform::capture(x, y, width, height, scale)
}

#[cfg(target_os = "windows")]
mod platform {
    use super::Screenshot;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, CAPTUREBLT, DIB_RGB_COLORS, SRCCOPY,
    };

    pub fn capture(x: i32, y: i32, width: u32, height: u32, _scale: f64) -> Result<Screenshot, String> {
        let (w, h) = (width as i32, height as i32);
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        // SAFETY: every handle is created here, checked, and released before returning.
        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return Err("GetDC failed".into());
            }
            let memory = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, w, h);
            let previous = SelectObject(memory, bitmap.into());
            let copied = BitBlt(memory, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT);
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            SelectObject(memory, previous);
            let lines = GetDIBits(
                memory,
                bitmap,
                0,
                height,
                Some(rgba.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            );
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            copied.map_err(|e| format!("BitBlt failed: {e}"))?;
            if lines == 0 {
                return Err("GetDIBits failed".into());
            }
        }
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        Ok(Screenshot { width, height, rgba })
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::Screenshot;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_core_graphics::{kCGNullWindowID, CGDataProvider, CGImage, CGWindowImageOption, CGWindowListOption};

    pub fn capture(x: i32, y: i32, width: u32, height: u32, scale: f64) -> Result<Screenshot, String> {
        let rect = CGRect::new(
            CGPoint::new(f64::from(x) / scale, f64::from(y) / scale),
            CGSize::new(f64::from(width) / scale, f64::from(height) / scale),
        );
        // ScreenCaptureKit replaces it, but the call still works and needs only the screen
        // recording permission Java's helper needed too.
        #[allow(deprecated)]
        let image = objc2_core_graphics::CGWindowListCreateImage(
            rect,
            CGWindowListOption::OptionOnScreenOnly,
            kCGNullWindowID,
            CGWindowImageOption::Default,
        )
        .ok_or("no screen picture (screen recording permission?)")?;
        let image = Some(&*image);
        let (iw, ih, row) = (
            CGImage::width(image),
            CGImage::height(image),
            CGImage::bytes_per_row(image),
        );
        let data = CGDataProvider::data(CGImage::data_provider(image).as_deref()).ok_or("no picture data")?;
        let bytes = data.to_vec();
        let mut rgba = Vec::with_capacity(iw * ih * 4);
        for line in bytes.chunks_exact(row).take(ih) {
            for px in line[..iw * 4].chunks_exact(4) {
                rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        Ok(Screenshot {
            width: iw as u32,
            height: ih as u32,
            rgba,
        })
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::Screenshot;

    pub fn capture(x: i32, y: i32, width: u32, height: u32, _scale: f64) -> Result<Screenshot, String> {
        if std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s.eq_ignore_ascii_case("wayland")) {
            portal(x, y, width, height)
        } else {
            x11(x, y, width, height)
        }
    }

    fn x11(x: i32, y: i32, width: u32, height: u32) -> Result<Screenshot, String> {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, ImageOrder};
        let (conn, screen) = x11rb::connect(None).map_err(|e| format!("X11: {e}"))?;
        let setup = conn.setup();
        let root = &setup.roots[screen];
        let reply = conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                root.root,
                x as i16,
                y as i16,
                width as u16,
                height as u16,
                !0,
            )
            .map_err(|e| format!("X11: {e}"))?
            .reply()
            .map_err(|e| format!("X11: {e}"))?;
        let bpp = setup
            .pixmap_formats
            .iter()
            .find(|f| f.depth == reply.depth)
            .map_or(32, |f| f.bits_per_pixel);
        if bpp != 32 || reply.depth < 24 {
            return Err(format!(
                "X11: unsupported pixel format (depth {}, {bpp} bits)",
                reply.depth
            ));
        }
        let lsb = setup.image_byte_order == ImageOrder::LSB_FIRST;
        let stride = reply.data.len() / height as usize;
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for line in reply.data.chunks_exact(stride).take(height as usize) {
            for px in line[..width as usize * 4].as_chunks::<4>().0 {
                // 0x00RRGGBB in the server's byte order.
                let (r, g, b) = if lsb {
                    (px[2], px[1], px[0])
                } else {
                    (px[1], px[2], px[3])
                };
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Ok(Screenshot { width, height, rgba })
    }

    /// `org.freedesktop.portal.Screenshot`: a non-interactive picture of the whole desktop as a
    /// PNG file, cut to the area.
    fn portal(x: i32, y: i32, width: u32, height: u32) -> Result<Screenshot, String> {
        use std::collections::HashMap;
        use zbus::blocking::{Connection, Proxy};
        use zbus::zvariant::{OwnedValue, Value};
        let err = |e: zbus::Error| format!("screenshot portal: {e}");
        let conn = Connection::session().map_err(err)?;
        let token = format!("lizzie{}", std::process::id());
        let sender = conn
            .unique_name()
            .ok_or("screenshot portal: no bus name")?
            .trim_start_matches(':')
            .replace('.', "_");
        let request = Proxy::new(
            &conn,
            "org.freedesktop.portal.Desktop",
            format!("/org/freedesktop/portal/desktop/request/{sender}/{token}"),
            "org.freedesktop.portal.Request",
        )
        .map_err(err)?;
        let mut responses = request.receive_signal("Response").map_err(err)?;
        let portal = Proxy::new(
            &conn,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Screenshot",
        )
        .map_err(err)?;
        let mut options: HashMap<&str, Value> = HashMap::new();
        options.insert("handle_token", Value::from(token.as_str()));
        options.insert("interactive", Value::from(false));
        let _: zbus::zvariant::OwnedObjectPath = portal.call("Screenshot", &("", options)).map_err(err)?;
        let message = responses.next().ok_or("screenshot portal: no answer")?;
        let (code, results): (u32, HashMap<String, OwnedValue>) = message.body().deserialize().map_err(err)?;
        if code != 0 {
            return Err(format!("screenshot portal: refused ({code})"));
        }
        let uri: String = results
            .get("uri")
            .and_then(|v| String::try_from(v.clone()).ok())
            .ok_or("screenshot portal: no picture")?;
        let path = file_path(&uri).ok_or("screenshot portal: not a file")?;
        let picture = image::open(&path).map_err(|e| format!("screenshot portal: {e}"));
        let _ = std::fs::remove_file(&path);
        let picture = picture?.to_rgba8();
        let (pw, ph) = picture.dimensions();
        let (x, y) = (x.clamp(0, pw as i32) as u32, y.clamp(0, ph as i32) as u32);
        let (width, height) = (width.min(pw - x), height.min(ph - y));
        let cut = image::imageops::crop_imm(&picture, x, y, width, height).to_image();
        Ok(Screenshot {
            width,
            height,
            rgba: cut.into_raw(),
        })
    }

    /// The local path of a `file://` URI.
    fn file_path(uri: &str) -> Option<std::path::PathBuf> {
        let encoded = uri.strip_prefix("file://")?;
        let bytes = encoded.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        use std::os::unix::ffi::OsStringExt;
        Some(std::ffi::OsString::from_vec(out).into())
    }
}

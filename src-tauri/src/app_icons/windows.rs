//! Extract embedded icons without executing the target program.
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path, ptr};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    UI::{Shell::ExtractIconExW, WindowsAndMessaging::*},
};
const SIZE: usize = 64;
struct Icon(HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            DestroyIcon(self.0);
        }
    }
}
struct Canvas {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}
impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
pub fn load(path: &str) -> Option<Vec<u8>> {
    // Do not contact a remote file server just to resolve an icon.
    if path.starts_with("\\\\") || !Path::new(path).is_file() {
        return None;
    }
    let wide: Vec<_> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut handle = ptr::null_mut();
        let count = ExtractIconExW(wide.as_ptr(), 0, &mut handle, ptr::null_mut(), 1);
        if count == 0 || count == u32::MAX || handle.is_null() {
            return None;
        }
        let icon = Icon(handle);
        let dc = CreateCompatibleDC(ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = SIZE as i32;
        info.bmiHeader.biHeight = -(SIZE as i32);
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits: *mut c_void = ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            DeleteDC(dc);
            return None;
        }
        let previous = SelectObject(dc, bitmap);
        if previous.is_null() || previous as isize == -1 {
            DeleteObject(bitmap);
            DeleteDC(dc);
            return None;
        }
        let _canvas = Canvas {
            dc,
            bitmap,
            previous,
        };
        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), SIZE * SIZE * 4);
        // Two backgrounds recover transparency for both alpha and older masked icons.
        pixels.fill(0);
        if DrawIconEx(
            dc,
            0,
            0,
            icon.0,
            SIZE as i32,
            SIZE as i32,
            0,
            ptr::null_mut(),
            DI_NORMAL,
        ) == 0
        {
            return None;
        }
        GdiFlush();
        let black = pixels.to_vec();
        pixels.fill(255);
        if DrawIconEx(
            dc,
            0,
            0,
            icon.0,
            SIZE as i32,
            SIZE as i32,
            0,
            ptr::null_mut(),
            DI_NORMAL,
        ) == 0
        {
            return None;
        }
        GdiFlush();
        let rgba = rgba(&black, pixels);
        let mut output = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut output, SIZE as u32, SIZE as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
        }
        Some(output)
    }
}
fn rgba(black: &[u8], white: &[u8]) -> Vec<u8> {
    black
        .chunks_exact(4)
        .zip(white.chunks_exact(4))
        .flat_map(|(b, w)| {
            let alpha = 255
                - (0..3)
                    .map(|i| w[i].saturating_sub(b[i]))
                    .max()
                    .unwrap_or(255);
            let channel = |i: usize| {
                if alpha == 0 {
                    0
                } else {
                    ((b[i] as u32 * 255 / alpha as u32).min(255)) as u8
                }
            };
            [channel(2), channel(1), channel(0), alpha]
        })
        .collect()
}
#[cfg(test)]
mod tests {
    #[test]
    fn converts_masked_and_alpha_icons_to_rgba() {
        assert_eq!(
            super::rgba(
                &[0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 128, 0],
                &[255, 255, 255, 0, 0, 0, 255, 0, 127, 127, 255, 0]
            ),
            [0, 0, 0, 0, 255, 0, 0, 255, 255, 0, 0, 128]
        );
    }
    #[test]
    fn extracts_system_program_and_falls_back_for_missing_files() {
        let root = std::env::var("SystemRoot").unwrap();
        let icon = super::load(&format!("{root}\\System32\\notepad.exe")).unwrap();
        assert!(icon.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(super::load("Z:\\missing-icon.exe").is_none());
    }
}

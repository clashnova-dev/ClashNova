//! Resolve icons only for processes already observed by the connection monitor.
#[cfg(any(target_os = "linux", test))]
#[path = "app_icons/linux.rs"]
mod linux;
#[cfg(windows)]
#[path = "app_icons/windows.rs"]
mod windows;
#[cfg(target_os = "macos")]
use std::path::Path;

// No backend cache: the webview owns the icon cache (see app-icon-cache.ts), so resolve the
// icon live on every call. The frontend only reaches this on a cache miss, and the system
// icon lookup is cheap, so a fresh fetch stays fast while always reflecting the current icon.
pub fn load(path: &str) -> Option<Vec<u8>> {
    native_icon(path)
}

fn native_icon(path: &str) -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
        use objc2_foundation::{NSDictionary, NSString};
        // Use the outer application rather than the browser's nested Helper.app.
        let bundle = path
            .find(".app/")
            .map(|end| &path[..end + 4])
            .unwrap_or(path);
        if !Path::new(bundle).exists() {
            return None;
        }
        objc2::rc::autoreleasepool(|_| {
            let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(bundle));
            let tiff = image.TIFFRepresentation()?;
            let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
            // Empty properties dictionary contains no untyped property values.
            let png = unsafe {
                bitmap.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            }?;
            let bytes = png.to_vec();
            (bytes.len() <= 2 * 1024 * 1024).then_some(bytes)
        })
    }
    #[cfg(windows)]
    {
        windows::load(path)
    }
    #[cfg(target_os = "linux")]
    {
        linux::load(path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = path;
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn reads_installed_application_png() {
        let data =
            super::load("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal")
                .unwrap();
        assert!(data.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(super::load("/does-not-exist.app/Contents/MacOS/app").is_none());
    }
}

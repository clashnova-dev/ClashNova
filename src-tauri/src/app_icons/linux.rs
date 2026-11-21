//! Best-effort XDG desktop-entry matching. Never execute an Exec command.
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
#[cfg(target_os = "linux")]
const MAX_FILE: u64 = 2 * 1024 * 1024;
fn read_text(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    use std::io::Read;
    let mut text = String::new();
    file.take(128 * 1024 + 1).read_to_string(&mut text).ok()?;
    (text.len() <= 128 * 1024).then_some(text)
}
fn entry(text: &str) -> Option<(String, String)> {
    let mut main = false;
    let (mut exec, mut icon, mut hidden, mut kind) = (None, None, false, None);
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            main = line == "[Desktop Entry]";
            continue;
        }
        if !main || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Exec" => exec = Some(value.trim().to_owned()),
            "Icon" => icon = Some(value.trim().to_owned()),
            "Hidden" => hidden = value.trim() == "true",
            "Type" => kind = Some(value.trim()),
            _ => {}
        }
    }
    if hidden || kind != Some("Application") {
        return None;
    }
    Some((executable(&exec?)?, icon?))
}
fn executable(exec: &str) -> Option<String> {
    // Desktop Exec uses double-quoted arguments and backslash escaping, not a shell.
    let (mut tokens, mut token, mut quote, mut escape) = (Vec::new(), String::new(), false, false);
    for ch in exec.chars() {
        if escape {
            token.push(ch);
            escape = false;
        } else if ch == '\\' {
            escape = true;
        } else if ch == '"' {
            quote = !quote;
        } else if ch.is_whitespace() && !quote {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(ch);
        }
    }
    if quote || escape {
        return None;
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    let first = tokens.first()?;
    if Path::new(first).file_name()?.to_str()? == "env" {
        tokens
            .iter()
            .skip(1)
            .find(|s| !s.contains('=') && !s.starts_with('-'))
            .cloned()
    } else {
        Some(first.clone())
    }
}
fn children(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .take(4096)
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths
}
fn icon_file(icon: &str, ext: &str) -> String {
    if [".png", ".svg", ".xpm"]
        .iter()
        .any(|suffix| icon.ends_with(suffix))
    {
        icon.to_owned()
    } else {
        format!("{icon}.{ext}")
    }
}
fn themed(icon: &str, roots: &[PathBuf], theme: &str) -> Option<PathBuf> {
    let path = Path::new(icon);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_owned());
    }
    if icon.is_empty() || icon.contains(['/', '\\']) || icon == "." || icon == ".." {
        return None;
    }
    let mut themes = vec![theme.to_owned(), "hicolor".into(), "Adwaita".into()];
    let mut seen = HashSet::new();
    // Follow declared inheritance, bounded to avoid cycles and unbounded directory walks.
    let mut index = 0;
    while index < themes.len() && index < 32 {
        let name = themes[index].clone();
        index += 1;
        if name.contains(['/', '\\']) || !seen.insert(name.clone()) {
            continue;
        }
        for root in roots {
            let dir = root.join("icons").join(&name);
            let config = read_text(&dir.join("index.theme")).unwrap_or_default();
            let mut directories: Vec<String> = ["64x64", "48x48", "32x32", "scalable"]
                .into_iter()
                .map(|size| format!("{size}/apps"))
                .collect();
            for line in config.lines() {
                if let Some(value) = line.strip_prefix("Inherits=") {
                    themes.splice(
                        index..index,
                        value.split(',').take(16).map(|s| s.trim().to_owned()),
                    );
                }
                if let Some(value) = line
                    .strip_prefix("Directories=")
                    .or_else(|| line.strip_prefix("ScaledDirectories="))
                {
                    directories.extend(value.split(',').take(256).map(str::to_owned));
                }
            }
            // Common hicolor layouts also work without an index installed.
            for size in [
                "48x48", "64x64", "32x32", "scalable", "128x128", "256x256", "24x24", "16x16",
            ] {
                directories.push(format!("{size}/apps"));
            }
            for sub in directories.iter().map(Path::new).filter(|p| {
                p.components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
            }) {
                for ext in ["png", "svg", "xpm"] {
                    let candidate = dir.join(sub).join(
                        if [".png", ".svg", ".xpm"]
                            .iter()
                            .any(|suffix| icon.ends_with(suffix))
                        {
                            icon.to_owned()
                        } else {
                            icon_file(icon, ext)
                        },
                    );
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
            for ext in ["png", "svg", "xpm"] {
                let candidate = dir.join(icon_file(icon, ext));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    for root in roots {
        for ext in ["png", "svg", "xpm"] {
            let unthemed = root.join("icons").join(icon_file(icon, ext));
            if unthemed.is_file() {
                return Some(unthemed);
            }
            let path = root.join("pixmaps").join(icon_file(icon, ext));
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}
fn locate(process: &Path, roots: &[PathBuf], theme: &str) -> Option<PathBuf> {
    let executable_name = process.file_name()?;
    let canonical = fs::canonicalize(process).ok();
    let mut fallback = Vec::new();
    for root in roots {
        for path in children(&root.join("applications")) {
            if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                continue;
            }
            let Some((exe, icon)) = read_text(&path).and_then(|s| entry(&s)) else {
                continue;
            };
            let exe = Path::new(&exe);
            if exe.is_absolute()
                && (exe == process
                    || canonical
                        .as_ref()
                        .is_some_and(|real| fs::canonicalize(exe).ok().as_ref() == Some(real)))
            {
                if let Some(icon) = themed(&icon, roots, theme) {
                    return Some(icon);
                }
            } else if exe.file_name() == Some(executable_name) {
                fallback.push(icon);
            }
        }
    }
    fallback.sort();
    fallback.dedup();
    // Avoid assigning a random Electron/browser icon when multiple launchers match.
    if fallback.len() == 1 {
        return themed(&fallback[0], roots, theme);
    }
    None
}
#[cfg(target_os = "linux")]
pub fn load(process: &str) -> Option<Vec<u8>> {
    let mut roots = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(root) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.as_ref().map(|h| h.join(".local/share")))
    {
        roots.push(root);
    }
    roots.extend(
        std::env::split_paths(
            &std::env::var_os("XDG_DATA_DIRS")
                .unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
        )
        .filter(|p| p.is_absolute()),
    );
    if let Some(home) = &home {
        roots.push(home.join(".local/share/flatpak/exports/share"));
    }
    roots.extend([
        PathBuf::from("/var/lib/flatpak/exports/share"),
        PathBuf::from("/var/lib/snapd/desktop"),
    ]);
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|h| h.join(".config")));
    let theme = config
        .and_then(|p| read_text(&p.join("gtk-3.0/settings.ini")))
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("gtk-icon-theme-name=")
                    .map(|v| v.trim().trim_matches('"').to_owned())
            })
        })
        .unwrap_or_else(|| "hicolor".into());
    let path = locate(Path::new(process), &roots, &theme)?;
    decode(&path)
}
#[cfg(target_os = "linux")]
fn decode(path: &Path) -> Option<Vec<u8>> {
    if fs::metadata(path).ok()?.len() > MAX_FILE {
        return None;
    }
    let image = gdk_pixbuf::Pixbuf::from_file_at_scale(path, 64, 64, true).ok()?;
    image.save_to_bufferv("png", &[]).ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn converts_svg_and_falls_back_for_invalid_images() {
        let dir = tempfile::tempdir().unwrap();
        let svg = dir.path().join("icon.svg");
        fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="blue"/></svg>"#).unwrap();
        assert!(decode(&svg).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
        fs::write(&svg, b"not an icon").unwrap();
        assert!(decode(&svg).is_none());
    }
    #[test]
    fn desktop_exec_is_parsed_without_running_it() {
        assert_eq!(
            executable("env MODE=1 \"/opt/My App/app\" %U"),
            Some("/opt/My App/app".into())
        );
        assert_eq!(entry("[Desktop Entry]\nType=Application\nExec=cursor %U\nIcon=cursor\n[Desktop Action New]\nIcon=wrong"), Some(("cursor".into(), "cursor".into())));
        assert!(
            entry("[Desktop Entry]\nType=Application\nHidden=true\nExec=app\nIcon=app").is_none()
        );
    }
    #[test]
    fn resolves_exact_theme_inheritance_and_ambiguous_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("applications")).unwrap();
        fs::create_dir_all(root.join("icons/hicolor/48x48/apps")).unwrap();
        fs::create_dir_all(root.join("icons/Custom")).unwrap();
        fs::write(
            root.join("icons/Custom/index.theme"),
            "[Icon Theme]\nInherits=hicolor\n",
        )
        .unwrap();
        let image = root.join("icons/hicolor/48x48/apps/app.png");
        fs::write(&image, b"fixture").unwrap();
        fs::write(
            root.join("applications/app.desktop"),
            "[Desktop Entry]\nType=Application\nExec=/opt/app/bin/app %U\nIcon=app",
        )
        .unwrap();
        assert_eq!(
            locate(Path::new("/opt/app/bin/app"), &[root.to_owned()], "Custom"),
            Some(image.clone())
        );
        fs::write(
            root.join("applications/other.desktop"),
            "[Desktop Entry]\nType=Application\nExec=/another/app %U\nIcon=other",
        )
        .unwrap();
        assert!(locate(Path::new("/unknown/app"), &[root.to_owned()], "Custom").is_none());
        assert!(themed("../private", &[root.to_owned()], "Custom").is_none());
        assert_eq!(
            themed(image.to_str().unwrap(), &[root.to_owned()], "Custom"),
            Some(image)
        );
    }
}

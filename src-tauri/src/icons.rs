//! Game icons: extracted from the exe (Windows shell API → PNG), Steam's
//! library icon, or a custom image chosen by the user. Stored as `icons/<game-id>.<ext>` in the app dir
//! and sent to the UI as `data:` URLs (no file-system scope needed).

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Largest custom image accepted.
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

/// What to do with a game's icon when saving it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IconChoice {
    /// Leave the current icon as is.
    #[default]
    Keep,
    /// Use the icon embedded in the game's exe.
    Exe,
    /// Use Steam's cached library icon (Steam games).
    Steam,
    /// Use an image file (png, jpg, gif, webp, bmp, ico).
    Custom { path: PathBuf },
    /// No icon.
    None,
}

fn image_mime(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

fn check_image(path: &Path) -> AppResult<&'static str> {
    let mime = image_mime(path)
        .ok_or_else(|| AppError::Other(format!("{} is not a supported image type", path.display())))?;
    let len = fs::metadata(path).map_err(|e| AppError::io(path, e))?.len();
    if len > MAX_IMAGE_BYTES {
        return Err(AppError::Other(format!("{} is larger than 5 MB", path.display())));
    }
    Ok(mime)
}

fn to_data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// `data:` URL of an icon file.
pub fn data_url(path: &Path) -> AppResult<String> {
    let mime = check_image(path)?;
    let bytes = fs::read(path).map_err(|e| AppError::io(path, e))?;
    Ok(to_data_url(mime, &bytes))
}

/// Where an icon can come from: the game's exe and Steam's library icon.
#[derive(Debug, Clone, Copy, Default)]
pub struct IconSources<'a> {
    pub exe: Option<&'a Path>,
    pub steam: Option<&'a Path>,
}

/// Preview of a choice before it is saved (`None` = no icon available).
pub fn preview(choice: &IconChoice, sources: IconSources) -> AppResult<Option<String>> {
    match choice {
        IconChoice::Exe => Ok(sources
            .exe
            .and_then(|exe| exe_icon_png(exe).ok())
            .map(|png| to_data_url("image/png", &png))),
        IconChoice::Steam => Ok(sources.steam.and_then(|icon| data_url(icon).ok())),
        IconChoice::Custom { path } => data_url(path).map(Some),
        IconChoice::Keep | IconChoice::None => Ok(None),
    }
}

/// Applies `choice` for `game_id`; returns the new icon path. The previous
/// icon file is removed once the new one is written. A missing exe or Steam
/// icon is not an error (the game simply has no icon).
pub fn apply(
    icons_dir: &Path,
    game_id: Uuid,
    choice: &IconChoice,
    sources: IconSources,
    current: Option<&Path>,
) -> AppResult<Option<PathBuf>> {
    let exe = sources.exe;
    let (bytes, ext) = match choice {
        IconChoice::Keep => return Ok(current.map(Path::to_path_buf)),
        IconChoice::None => {
            remove(current);
            return Ok(None);
        }
        IconChoice::Exe => match exe.map(exe_icon_png) {
            Some(Ok(png)) => (png, "png".to_string()),
            Some(Err(e)) => {
                eprintln!("could not extract the exe icon: {e}");
                remove(current);
                return Ok(None);
            }
            None => {
                remove(current);
                return Ok(None);
            }
        },
        IconChoice::Steam => match sources.steam.map(|p| check_image(p).and_then(|_| read_image(p))) {
            Some(Ok(image)) => image,
            _ => {
                remove(current);
                return Ok(None);
            }
        },
        IconChoice::Custom { path } => {
            check_image(path)?;
            read_image(path)?
        }
    };
    fs::create_dir_all(icons_dir).map_err(|e| AppError::io(icons_dir, e))?;
    let target = icons_dir.join(format!("{game_id}.{ext}"));
    fs::write(&target, bytes).map_err(|e| AppError::io(&target, e))?;
    if current.is_some_and(|c| c != target) {
        remove(current);
    }
    Ok(Some(target))
}

fn read_image(path: &Path) -> AppResult<(Vec<u8>, String)> {
    let bytes = fs::read(path).map_err(|e| AppError::io(path, e))?;
    let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
    Ok((bytes, ext))
}

/// Deletes an icon file (best effort).
pub fn remove(icon: Option<&Path>) {
    if let Some(icon) = icon {
        let _ = fs::remove_file(icon);
    }
}

/// Extracts the main icon of an exe (or .ico/.dll) as a 256px PNG.
#[cfg(windows)]
pub fn exe_icon_png(exe: &Path) -> AppResult<Vec<u8>> {
    let (width, height, rgba) = win::extract_rgba(exe, 256)
        .map_err(|e| AppError::Other(format!("cannot extract the icon of {}: {e}", exe.display())))?;
    encode_png(width, height, &rgba)
}

#[cfg(not(windows))]
pub fn exe_icon_png(exe: &Path) -> AppResult<Vec<u8>> {
    Err(AppError::Other(format!("icon extraction is not supported on this platform ({})", exe.display())))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let err = |e: png::EncodingError| AppError::Other(format!("PNG encoding failed: {e}"));
    let mut writer = encoder.write_header().map_err(err)?;
    writer.write_image_data(rgba).map_err(err)?;
    writer.finish().map_err(err)?;
    Ok(out)
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::Shell::SHDefExtractIconW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    /// Returns (width, height, RGBA pixels).
    pub fn extract_rgba(path: &Path, size: u32) -> Result<(u32, u32, Vec<u8>), String> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut icon = HICON::default();
        // SAFETY: `wide` is NUL-terminated and outlives the call; `icon` is a valid out pointer.
        let hr = unsafe { SHDefExtractIconW(PCWSTR(wide.as_ptr()), 0, 0, Some(&mut icon), None, size) };
        if hr.is_err() || icon.is_invalid() {
            return Err("the file has no icon".into());
        }
        // SAFETY: `icon` is a valid icon handle owned by us; destroyed below.
        let result = unsafe { icon_to_rgba(icon) };
        // SAFETY: as above.
        let _ = unsafe { DestroyIcon(icon) };
        result
    }

    unsafe fn icon_to_rgba(icon: HICON) -> Result<(u32, u32, Vec<u8>), String> {
        let mut info = ICONINFO::default();
        unsafe { GetIconInfo(icon, &mut info) }.map_err(|e| e.to_string())?;
        let color = info.hbmColor;
        let mask = info.hbmMask;
        let result = (|| {
            if color.is_invalid() {
                return Err("monochrome icons are not supported".to_string());
            }
            let mut bmp = BITMAP::default();
            let n = unsafe {
                GetObjectW(
                    color.into(),
                    std::mem::size_of::<BITMAP>() as i32,
                    Some(&mut bmp as *mut BITMAP as *mut _),
                )
            };
            if n == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
                return Err("cannot read the icon bitmap".to_string());
            }
            let (w, h) = (bmp.bmWidth as u32, bmp.bmHeight as u32);
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32), // top-down rows
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            let dc = unsafe { CreateCompatibleDC(None) };
            let lines = unsafe {
                GetDIBits(
                    dc,
                    color,
                    0,
                    h,
                    Some(pixels.as_mut_ptr() as *mut _),
                    &mut bi,
                    DIB_RGB_COLORS,
                )
            };
            let _ = unsafe { DeleteDC(dc) };
            if lines == 0 {
                return Err("cannot read the icon pixels".to_string());
            }
            // BGRA -> RGBA; icons without an alpha channel are made opaque.
            let has_alpha = pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
            for p in pixels.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
                if !has_alpha {
                    p[3] = 255;
                }
            }
            Ok((w, h, pixels))
        })();
        unsafe {
            if !color.is_invalid() {
                let _ = DeleteObject(color.into());
            }
            if !mask.is_invalid() {
                let _ = DeleteObject(mask.into());
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(windows)]
    #[test]
    fn extracts_png_from_a_system_exe() {
        let windir = std::env::var_os("SystemRoot").expect("SystemRoot");
        let exe = Path::new(&windir).join("explorer.exe");
        let png = exe_icon_png(&exe).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn apply_custom_then_none_and_keep() {
        let tmp = tempdir().unwrap();
        let icons = tmp.path().join("icons");
        let image = tmp.path().join("cover.JPG");
        fs::write(&image, b"jpeg bytes").unwrap();
        let game = Uuid::new_v4();

        let none = IconSources::default();
        let icon = apply(&icons, game, &IconChoice::Custom { path: image.clone() }, none, None)
            .unwrap()
            .unwrap();
        assert_eq!(icon, icons.join(format!("{game}.jpg")));
        assert!(data_url(&icon).unwrap().starts_with("data:image/jpeg;base64,"));

        assert_eq!(apply(&icons, game, &IconChoice::Keep, none, Some(&icon)).unwrap(), Some(icon.clone()));
        assert_eq!(apply(&icons, game, &IconChoice::None, none, Some(&icon)).unwrap(), None);
        assert!(!icon.exists());

        let bad = tmp.path().join("notes.txt");
        fs::write(&bad, b"x").unwrap();
        assert!(apply(&icons, game, &IconChoice::Custom { path: bad }, none, None).is_err());
    }

    #[test]
    fn missing_exe_icon_is_not_an_error() {
        let tmp = tempdir().unwrap();
        let exe = tmp.path().join("not-an-exe.exe");
        fs::write(&exe, b"nope").unwrap();
        let sources = IconSources {
            exe: Some(&exe),
            steam: None,
        };
        let icons = tmp.path().join("icons");
        assert_eq!(apply(&icons, Uuid::new_v4(), &IconChoice::Exe, sources, None).unwrap(), None);
        assert_eq!(apply(&icons, Uuid::new_v4(), &IconChoice::Steam, sources, None).unwrap(), None);
    }

    #[test]
    fn steam_icon_is_copied() {
        let tmp = tempdir().unwrap();
        let cached = tmp.path().join("fccb20f309ac5ae05b3cb4576a8a3357edf86f2e.jpg");
        fs::write(&cached, b"jpeg").unwrap();
        let sources = IconSources {
            exe: None,
            steam: Some(&cached),
        };
        let game = Uuid::new_v4();
        let icon = apply(&tmp.path().join("icons"), game, &IconChoice::Steam, sources, None)
            .unwrap()
            .unwrap();
        assert_eq!(icon.file_name().unwrap().to_string_lossy(), format!("{game}.jpg"));
        assert_eq!(fs::read(&icon).unwrap(), b"jpeg");
        assert!(preview(&IconChoice::Steam, sources).unwrap().unwrap().starts_with("data:image/jpeg"));
    }
}

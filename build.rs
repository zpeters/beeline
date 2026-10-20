//! Prepares the app icon from `assets/icon/`:
//!
//! - `icon.rgba` in `OUT_DIR`: raw pixels of the 128 px PNG, which
//!   `main.rs` hands to the window (title bar and taskbar).
//! - On Windows, `beeline.ico` in `OUT_DIR`, embedded as the exe's icon.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// The PNGs packed into the .ico, with their pixel size.
const ICO_SIZES: [(&str, u32); 4] = [
    ("32x32.png", 32),
    ("32x32@2x.png", 64),
    ("128x128.png", 128),
    ("256x256.png", 256),
];

fn main() {
    println!("cargo:rerun-if-changed=assets/icon");
    let icons = Path::new("assets/icon");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    fs::write(out.join("icon.rgba"), rgba(&icons.join("128x128.png"))).unwrap();

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let ico = out.join("beeline.ico");
        fs::write(&ico, pack_ico(icons)).unwrap();
        embed_icon(&ico);
    }
}

/// The PNG's pixels as 8-bit RGBA.
fn rgba(path: &Path) -> Vec<u8> {
    let mut decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(path).unwrap()));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut buf).unwrap();
    buf.truncate(frame.buffer_size());
    match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("{}: unsupported colour type {other:?}", path.display()),
    }
}

/// An .ico holding the PNGs as they are (Windows Vista and later read
/// PNG entries), so there is no second icon file to keep in step.
fn pack_ico(icons: &Path) -> Vec<u8> {
    let images: Vec<(Vec<u8>, u32)> = ICO_SIZES
        .iter()
        .map(|(name, size)| (fs::read(icons.join(name)).unwrap(), *size))
        .collect();
    let count = images.len() as u16;
    let mut ico = Vec::new();
    // ICONDIR: reserved, type 1 (icon), image count.
    ico.extend(0u16.to_le_bytes());
    ico.extend(1u16.to_le_bytes());
    ico.extend(count.to_le_bytes());
    let mut offset = 6 + 16 * u32::from(count);
    for (data, size) in &images {
        // ICONDIRENTRY: a size of 256 is written as 0.
        let side = if *size >= 256 { 0 } else { *size as u8 };
        ico.extend([side, side, 0, 0]);
        ico.extend(1u16.to_le_bytes()); // colour planes
        ico.extend(32u16.to_le_bytes()); // bits per pixel
        ico.extend((data.len() as u32).to_le_bytes());
        ico.extend(offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (data, _) in images {
        ico.extend(data);
    }
    ico
}

#[cfg(windows)]
fn embed_icon(ico: &Path) {
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico.to_str().unwrap());
    res.compile().unwrap();
}

/// Cross-compiling for Windows from another OS: the exe gets no icon.
#[cfg(not(windows))]
fn embed_icon(_ico: &Path) {}

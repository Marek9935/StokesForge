//! Rasterises the two icon sources (application icon, project-file icon)
//! into what a release ships (#388): `hicolor/<n>x<n>/apps/stokesforge.png`,
//! `hicolor/<n>x<n>/mimetypes/application-x-stokesforge-project.png` and two
//! `.ico` files. Nothing it writes is committed: the scripts call it at each
//! release, from the SVG under `assets/logo/00-final/`.
//!
//! Before rasterising, [`check_source`] locks the framing the user validated
//! (transparent background, centred drawing, no tile) and refuses the sources
//! the rasteriser draws wrongly (`feDisplacementMap`, `feImage`).
//!
//! Output is byte-for-byte reproducible: no timestamp, no metadata.

use std::fmt;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

/// Sizes of the PNG trees.
pub const SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];
/// Sizes stored in each `.ico` (256 is the largest a frame can declare).
pub const ICO_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];
/// Icon name of the application (`stokesforge.desktop`).
pub const APP_ICON_NAME: &str = "stokesforge";
/// Icon name of the project file type: the MIME type with `/` replaced by `-`.
pub const PROJECT_ICON_NAME: &str = "application-x-stokesforge-project";
/// File name of the application `.ico`.
pub const APP_ICO: &str = "stokesforge.ico";
/// File name of the project-file `.ico`.
pub const PROJECT_ICO: &str = "stokesforge-project.ico";

/// Resolution at which the framing is measured.
const CHECK_SIZE: u32 = 256;
/// A pixel counts as drawn above this alpha (the soft shadow tail does not).
const DRAWN_ALPHA: u8 = 40;
/// Largest accepted distance between the centre of the drawn area and the
/// centre of the image, in pixels of the check resolution.
const CENTRE_TOLERANCE: f32 = 4.0;
/// Elements the rasteriser draws wrongly (`assets/logo/LISEZMOI.md`, section 6).
const FORBIDDEN: [&str; 2] = ["feDisplacementMap", "feImage"];

/// Why a source was refused or an output could not be written.
#[derive(Debug)]
pub enum Error {
    Read(PathBuf, std::io::Error),
    Write(PathBuf, std::io::Error),
    Parse(String),
    NotSquare(f32, f32),
    Forbidden(&'static str),
    Empty,
    OffCentre(f32, f32),
    OpaqueRing,
    Encode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Read(p, e) => write!(f, "cannot read {}: {e}", p.display()),
            Error::Write(p, e) => write!(f, "cannot write {}: {e}", p.display()),
            Error::Parse(e) => write!(f, "cannot parse the SVG: {e}"),
            Error::NotSquare(w, h) => write!(f, "the SVG must be square, it is {w}x{h}"),
            Error::Forbidden(name) => {
                write!(f, "the source uses `{name}`, which the rasteriser draws wrongly")
            }
            Error::Empty => write!(f, "the source draws nothing"),
            Error::OffCentre(dx, dy) => write!(
                f,
                "the drawing is off-centre by ({dx:.1}, {dy:.1}) px at {CHECK_SIZE} px"
            ),
            Error::OpaqueRing => write!(
                f,
                "the outer edge is drawn (tile or full bleed): the icon must have a transparent margin"
            ),
            Error::Encode(e) => write!(f, "cannot encode an image: {e}"),
        }
    }
}

impl std::error::Error for Error {}

fn parse(svg: &str) -> Result<resvg::usvg::Tree, Error> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .map_err(|e| Error::Parse(e.to_string()))?;
    let size = tree.size();
    if (size.width() - size.height()).abs() > 0.01 {
        return Err(Error::NotSquare(size.width(), size.height()));
    }
    Ok(tree)
}

/// Rasterises the SVG into a `size` x `size` image with straight
/// (not premultiplied) alpha.
pub fn render(svg: &str, size: u32) -> Result<RgbaImage, Error> {
    render_tree(&parse(svg)?, size)
}

fn render_tree(tree: &resvg::usvg::Tree, size: u32) -> Result<RgbaImage, Error> {
    let scale = size as f32 / tree.size().width();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| Error::Encode(format!("invalid size {size}")))?;
    resvg::render(
        tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied colour; PNG and ICO want straight colour.
    let mut data = Vec::with_capacity(pixmap.pixels().len() * 4);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        data.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    RgbaImage::from_raw(size, size, data).ok_or_else(|| Error::Encode("pixel count".into()))
}

/// Refuses a source whose framing or content would give a wrong icon.
pub fn check_source(svg: &str) -> Result<(), Error> {
    for name in FORBIDDEN {
        if svg.contains(name) {
            return Err(Error::Forbidden(name));
        }
    }
    let image = render(svg, CHECK_SIZE)?;
    let n = CHECK_SIZE;
    // The outer ring (2 px): a tile or a full-bleed background draws it.
    let on_ring = |x: u32, y: u32| x < 2 || y < 2 || x >= n - 2 || y >= n - 2;
    let (mut x0, mut y0, mut x1, mut y1) = (n, n, 0, 0);
    for (x, y, p) in image.enumerate_pixels() {
        if p.0[3] <= DRAWN_ALPHA {
            continue;
        }
        if on_ring(x, y) {
            return Err(Error::OpaqueRing);
        }
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + 1);
        y1 = y1.max(y + 1);
    }
    if x1 == 0 {
        return Err(Error::Empty);
    }
    let half = n as f32 / 2.0;
    let dx = (x0 + x1) as f32 / 2.0 - half;
    let dy = (y0 + y1) as f32 / 2.0 - half;
    if dx.abs() > CENTRE_TOLERANCE || dy.abs() > CENTRE_TOLERANCE {
        return Err(Error::OffCentre(dx, dy));
    }
    Ok(())
}

fn png_bytes(image: &RgbaImage) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    PngEncoder::new(Cursor::new(&mut out))
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| Error::Encode(e.to_string()))?;
    Ok(out)
}

fn ico_bytes(tree: &resvg::usvg::Tree) -> Result<Vec<u8>, Error> {
    let mut frames = Vec::new();
    for size in ICO_SIZES {
        let image = render_tree(tree, size)?;
        frames.push(
            IcoFrame::as_png(image.as_raw(), size, size, ExtendedColorType::Rgba8)
                .map_err(|e| Error::Encode(e.to_string()))?,
        );
    }
    let mut out = Vec::new();
    IcoEncoder::new(Cursor::new(&mut out))
        .encode_images(&frames)
        .map_err(|e| Error::Encode(e.to_string()))?;
    Ok(out)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::Write(parent.to_path_buf(), e))?;
    }
    fs::write(path, bytes).map_err(|e| Error::Write(path.to_path_buf(), e))
}

/// Writes one PNG tree: `out/hicolor/<n>x<n>/<category>/<name>.png`.
fn write_tree(
    tree: &resvg::usvg::Tree,
    out: &Path,
    category: &str,
    name: &str,
) -> Result<(), Error> {
    for size in SIZES {
        let image = render_tree(tree, size)?;
        let path = out
            .join("hicolor")
            .join(format!("{size}x{size}"))
            .join(category)
            .join(format!("{name}.png"));
        write(&path, &png_bytes(&image)?)?;
    }
    Ok(())
}

fn read(path: &Path) -> Result<String, Error> {
    fs::read_to_string(path).map_err(|e| Error::Read(path.to_path_buf(), e))
}

/// Checks both sources, then writes the two PNG trees and the two `.ico` files
/// under `out`. Nothing is written when a source is refused.
pub fn generate(icon: &Path, project: &Path, out: &Path) -> Result<(), Error> {
    let (icon_svg, project_svg) = (read(icon)?, read(project)?);
    check_source(&icon_svg)?;
    check_source(&project_svg)?;
    let (icon_tree, project_tree) = (parse(&icon_svg)?, parse(&project_svg)?);
    write_tree(&icon_tree, out, "apps", APP_ICON_NAME)?;
    write_tree(&project_tree, out, "mimetypes", PROJECT_ICON_NAME)?;
    write(&out.join(APP_ICO), &ico_bytes(&icon_tree)?)?;
    write(&out.join(PROJECT_ICO), &ico_bytes(&project_tree)?)?;
    Ok(())
}

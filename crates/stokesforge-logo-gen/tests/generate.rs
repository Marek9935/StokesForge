//! Tests of the icon generator on synthetic sources (never the real logo:
//! the real sources are checked by the CI job `linux-packages`, which runs the
//! binary on them).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// Literals on purpose: the tests state the contract the packaging scripts rely on.
const SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];
const ICO_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];
const APP_ICON_NAME: &str = "stokesforge";
const PROJECT_ICON_NAME: &str = "application-x-stokesforge-project";
const APP_ICO: &str = "stokesforge.ico";
const PROJECT_ICO: &str = "stokesforge-project.ico";

fn svg(body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">{body}</svg>"#
    )
}

/// A centred square with a transparent margin.
fn good() -> String {
    svg(r##"<rect x="24" y="24" width="208" height="208" fill="#336699"/>"##)
}

fn run(icon: &Path, project: &Path, out: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stokesforge-logo-gen"))
        .arg("--icon")
        .arg(icon)
        .arg("--project")
        .arg(project)
        .arg("--out")
        .arg(out)
        .output()
        .expect("run the generator")
}

/// Writes the two sources, runs the generator, returns the output.
fn generate(dir: &Path, icon: &str, project: &str) -> (Output, PathBuf) {
    let (i, p, o) = (dir.join("icon.svg"), dir.join("project.svg"), dir.join("out"));
    fs::write(&i, icon).unwrap();
    fs::write(&p, project).unwrap();
    (run(&i, &p, &o), o)
}

fn ok(dir: &Path, icon: &str, project: &str) -> PathBuf {
    let (output, out) = generate(dir, icon, project);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    out
}

fn png(path: &Path) -> image::RgbaImage {
    image::open(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .to_rgba8()
}

fn files(root: &Path, into: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, into);
        } else {
            into.push(path);
        }
    }
}

#[test]
fn every_size_is_written_in_both_trees_with_exact_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let out = ok(dir.path(), &good(), &good());
    for size in SIZES {
        for (category, name) in [("apps", APP_ICON_NAME), ("mimetypes", PROJECT_ICON_NAME)] {
            let path = out
                .join("hicolor")
                .join(format!("{size}x{size}"))
                .join(category)
                .join(format!("{name}.png"));
            assert_eq!(png(&path).dimensions(), (size, size), "{}", path.display());
        }
    }
    assert!(out.join(APP_ICO).is_file() && out.join(PROJECT_ICO).is_file());
}

#[test]
fn half_opacity_gives_half_alpha_with_unscaled_colour() {
    let dir = tempfile::tempdir().unwrap();
    let half = svg(r##"<rect x="24" y="24" width="208" height="208" fill="#ff0000" fill-opacity=".5"/>"##);
    let out = ok(dir.path(), &half, &good());
    let image = png(&out.join("hicolor/64x64/apps/stokesforge.png"));
    let [r, g, b, a] = image.get_pixel(32, 32).0;
    assert!((120..=136).contains(&a), "alpha {a}");
    assert!(r >= 250 && g <= 5 && b <= 5, "premultiplied colour leaked: {r},{g},{b}");
}

#[test]
fn ico_holds_one_png_frame_per_size_and_stores_256_as_zero() {
    let dir = tempfile::tempdir().unwrap();
    let out = ok(dir.path(), &good(), &good());
    for name in [APP_ICO, PROJECT_ICO] {
        let bytes = fs::read(out.join(name)).unwrap();
        assert_eq!(&bytes[..4], &[0, 0, 1, 0], "ICONDIR: reserved 0, type 1");
        let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
        assert_eq!(count, ICO_SIZES.len(), "{name}");
        for (i, size) in ICO_SIZES.iter().enumerate() {
            let entry = &bytes[6 + i * 16..6 + (i + 1) * 16];
            let declared = |b: u8| if b == 0 { 256 } else { u32::from(b) };
            assert_eq!((declared(entry[0]), declared(entry[1])), (*size, *size));
            if *size == 256 {
                assert_eq!((entry[0], entry[1]), (0, 0), "256 is stored as 0");
            }
            let len = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
            let at = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
            let frame = image::load_from_memory_with_format(
                &bytes[at..at + len],
                image::ImageFormat::Png,
            )
            .unwrap();
            assert_eq!((frame.width(), frame.height()), (*size, *size), "{name}");
        }
        assert!(ICO_SIZES.iter().all(|s| *s != 512));
    }
}

#[test]
fn two_runs_write_identical_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    let first = ok(&a, &good(), &good());
    let second = ok(&b, &good(), &good());
    let mut list = Vec::new();
    files(&first, &mut list);
    assert_eq!(list.len(), SIZES.len() * 2 + 2);
    for path in list {
        let other = second.join(path.strip_prefix(&first).unwrap());
        assert_eq!(fs::read(&path).unwrap(), fs::read(&other).unwrap(), "{}", path.display());
    }
}

fn refused(dir: &Path, icon: &str, expected: &str) {
    let (output, out) = generate(dir, icon, &good());
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(expected), "stderr: {stderr}");
    assert!(!out.exists(), "nothing is written when a source is refused");
}

#[test]
fn an_off_centre_drawing_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let left = svg(r##"<rect x="24" y="24" width="120" height="208" fill="#336699"/>"##);
    refused(dir.path(), &left, "off-centre");
}

#[test]
fn an_opaque_outer_ring_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let tile = svg(r##"<rect width="256" height="256" fill="#f3f1ec"/><rect x="24" y="24" width="208" height="208" fill="#336699"/>"##);
    refused(dir.path(), &tile, "transparent margin");
}

#[test]
fn a_source_with_a_displacement_map_or_an_image_filter_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    for element in ["feDisplacementMap", "feImage"] {
        let source = svg(&format!(
            r##"<filter id="f"><{element}/></filter><rect x="24" y="24" width="208" height="208" fill="#336699"/>"##
        ));
        refused(dir.path(), &source, element);
    }
}

#[test]
fn a_missing_path_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("icon.svg");
    fs::write(&existing, good()).unwrap();
    let output = run(&existing, &dir.path().join("missing.svg"), &dir.path().join("out"));
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing.svg"));
}

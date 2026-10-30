//! Comparaison avec FFmpeg, image par image et octet par octet (même IDCT :
//! `-idct simple`). Les vidéos de test sont fabriquées par FFmpeg ; sans
//! FFmpeg (variable `FFMPEG`, sinon `ffmpeg` du PATH), les tests passent sans
//! rien vérifier. `PS2_VIDEO_PSS` : des fichiers PSS à comparer aussi
//! (séparés comme dans le PATH : `;` sous Windows, `:` ailleurs).

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ps2_video::{Decoder, Frame, Movie};

fn ffmpeg() -> Option<String> {
    let exe = std::env::var("FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
    Command::new(&exe).arg("-version").output().ok()?.status.success().then_some(exe)
}

fn run(exe: &str, args: &[&str]) {
    let out = Command::new(exe).args(["-v", "error", "-y"]).args(args).output().unwrap();
    assert!(out.status.success(), "ffmpeg {args:?}\n{}", String::from_utf8_lossy(&out.stderr));
}

fn work() -> PathBuf {
    let dir = std::env::temp_dir().join("ps2-video-tests");
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Compare nos images à celles que FFmpeg décode de `input` (YUV 4:2:0 brut,
/// lu au fil de l'eau) ; rend le nombre d'images.
fn compare(exe: &str, input: &Path, ours: impl Iterator<Item = Frame>) -> usize {
    let mut child = Command::new(exe)
        .args(["-v", "error", "-idct", "simple", "-i"])
        .arg(input)
        .args(["-f", "rawvideo", "-pix_fmt", "yuv420p", "-fps_mode", "passthrough", "pipe:1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut theirs = child.stdout.take().unwrap();
    let name = input.display();
    // Toutes les images fausses (nombre d'octets et premiers écarts).
    let mut bad = Vec::new();
    let mut count = 0;
    for (n, f) in ours.enumerate() {
        count += 1;
        let mine: Vec<u8> = [&f.y[..], &f.cb, &f.cr].concat();
        let mut r = vec![0; mine.len()];
        if theirs.read_exact(&mut r).is_err() {
            bad.push(format!("image {n} : FFmpeg n'en a pas autant"));
            break;
        }
        let diff: Vec<usize> = (0..mine.len()).filter(|&i| mine[i] != r[i]).collect();
        if diff.is_empty() {
            continue;
        }
        let place = |i: usize| {
            let (plane, at, w) = match i.checked_sub(f.y.len()) {
                None => ("Y", i, f.width),
                Some(c) => (if c < f.cb.len() { "Cb" } else { "Cr" }, c % f.cb.len(), f.chroma_size().0),
            };
            format!("{plane}({},{}) {}≠{}", at % w, at / w, mine[i], r[i])
        };
        let first: Vec<String> = diff.iter().take(12).map(|&i| place(i)).collect();
        bad.push(format!("image {n} ({:?}) : {} octets ; {}", f.kind, diff.len(), first.join(" ")));
    }
    let mut rest = Vec::new();
    theirs.read_to_end(&mut rest).unwrap();
    child.wait().unwrap();
    if !rest.is_empty() {
        bad.push(format!("FFmpeg a {} octets d'images en plus", rest.len()));
    }
    assert!(bad.is_empty(), "{name} : {} écarts\n{}", bad.len(), bad.join("\n"));
    count
}

fn elementary(path: &Path) -> Vec<Frame> {
    let data = fs::read(path).unwrap();
    let mut d = Decoder::new();
    let mut frames = Vec::new();
    for chunk in data.chunks(3000) {
        d.push(chunk, None);
        frames.extend(std::iter::from_fn(|| d.frame()));
    }
    d.flush();
    frames.extend(std::iter::from_fn(|| d.frame()));
    frames
}

fn case(name: &str, size: &str, args: &[&str], ext: &str) {
    let Some(exe) = ffmpeg() else {
        eprintln!("FFmpeg absent : {name} non vérifié");
        return;
    };
    let path = work().join(format!("{name}.{ext}"));
    let src = format!("testsrc2=size={size}:rate=25:duration=2");
    let mut all = vec!["-f", "lavfi", "-i", &src];
    all.extend(args);
    all.push(path.to_str().unwrap());
    run(&exe, &all);
    compare(&exe, &path, elementary(&path).into_iter());
}

#[test]
fn mpeg2_progressive() {
    case("progressive", "352x288", &["-c:v", "mpeg2video", "-g", "12", "-bf", "2", "-b:v", "2M"], "m2v");
}

#[test]
fn mpeg2_interlaced() {
    case(
        "interlaced",
        "320x240",
        &[
            "-c:v",
            "mpeg2video",
            "-flags",
            "+ildct+ilme",
            "-top",
            "1",
            "-alternate_scan",
            "1",
            "-intra_vlc",
            "1",
            "-non_linear_quant",
            "1",
            "-qmax",
            "28",
            "-dc",
            "10",
            "-g",
            "15",
            "-bf",
            "2",
            "-b:v",
            "1500k",
        ],
        "m2v",
    );
}

#[test]
fn mpeg2_odd_size_coarse() {
    case("odd", "200x150", &["-c:v", "mpeg2video", "-bf", "1", "-q:v", "28"], "m2v");
}

#[test]
fn mpeg1() {
    case("mpeg1", "352x240", &["-c:v", "mpeg1video", "-bf", "2", "-b:v", "1M"], "m1v");
}

#[test]
fn pss_files() {
    let (Some(exe), Some(list)) = (ffmpeg(), std::env::var_os("PS2_VIDEO_PSS")) else { return };
    for path in std::env::split_paths(&list).filter(|p| !p.as_os_str().is_empty()) {
        let mut movie = Movie::new(BufReader::new(File::open(&path).unwrap()));
        let frames = std::iter::from_fn(|| movie.frame().unwrap());
        let n = compare(&exe, &path, frames);
        eprintln!("{} : {n} images identiques", path.display());
    }
}

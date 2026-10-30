//! Décode une vidéo PSS en entier et affiche ce qu'elle contient, avec la
//! vitesse de décodage. Usage : cargo run --release --example pss_info FICHIER.PSS

use std::fs::File;
use std::io::BufReader;
use std::time::Instant;

fn main() -> std::io::Result<()> {
    let path = std::env::args().nth(1).expect("usage : pss_info FICHIER.PSS");
    let mut movie = ps2_video::Movie::new(BufReader::new(File::open(&path)?));
    let start = Instant::now();
    let (mut frames, mut samples) = (0usize, 0usize);
    let mut last_pts = None;
    while let Some(f) = movie.frame()? {
        frames += 1;
        samples += movie.take_samples().len();
        last_pts = f.pts.or(last_pts);
    }
    let took = start.elapsed().as_secs_f64();
    let seq = movie.sequence().expect("pas de vidéo");
    let (n, d) = seq.frame_rate;
    let fps = f64::from(n) / f64::from(d);
    println!("{path}");
    println!(
        "vidéo : {}×{}, {}, {}, {fps:.3} images/s, affichage {:.3}:1",
        seq.width,
        seq.height,
        if seq.mpeg2 { "MPEG-2" } else { "MPEG-1" },
        if seq.progressive { "progressive" } else { "entrelacée" },
        seq.display_aspect()
    );
    println!("{frames} images ({:.1} s), dernier horodatage {:?}", frames as f64 / fps, last_pts);
    match movie.audio() {
        Some(a) => println!("son : {:?}, {} Hz, {} voies, {} s", a.codec, a.rate, a.channels, samples / a.channels / a.rate as usize),
        None => println!("pas de son"),
    }
    println!("décodée en {took:.2} s ({:.0} images/s)", frames as f64 / took);
    Ok(())
}

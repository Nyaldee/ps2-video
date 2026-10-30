//! Vidéos de la PlayStation 2, en Rust pur : conteneur PSS, vidéo MPEG-1 et
//! MPEG-2, son PCM et ADPCM.
//!
//! ```no_run
//! let file = std::io::BufReader::new(std::fs::File::open("OP.PSS")?);
//! let mut movie = ps2_video::Movie::new(file);
//! while let Some(frame) = movie.frame()? {
//!     let rgba = frame.rgba();
//!     let samples = movie.take_samples();
//!     # let _ = (rgba, samples);
//! }
//! # Ok::<(), std::io::Error>(())
//! ```

mod audio;
mod bits;
mod idct;
mod mpeg;
mod pss;
mod tables;

use std::io::{self, Read};

pub use audio::{AudioDecoder, AudioInfo, Codec};
pub use mpeg::{Decoder, Frame, PictureType, Sequence};
pub use pss::{Demuxer, Packet, Stream};

/// Une vidéo PSS : la vidéo et le son du premier flux de chaque.
pub struct Movie<R> {
    demux: Demuxer<R>,
    video: Decoder,
    audio: AudioDecoder,
    samples: Vec<i16>,
    end: bool,
}

impl<R: Read> Movie<R> {
    pub fn new(src: R) -> Self {
        Self { demux: Demuxer::new(src), video: Decoder::new(), audio: AudioDecoder::new(), samples: Vec::new(), end: false }
    }

    /// Image suivante ; le son lu en chemin s'accumule (`take_samples`).
    pub fn frame(&mut self) -> io::Result<Option<Frame>> {
        loop {
            if let Some(f) = self.video.frame() {
                return Ok(Some(f));
            }
            if self.end {
                return Ok(None);
            }
            match self.demux.next_packet()? {
                Some(p) => match p.stream {
                    Stream::Video(0) => self.video.push(&p.data, p.pts),
                    Stream::Audio(0) => self.audio.push(&p.data, &mut self.samples),
                    _ => {}
                },
                None => {
                    self.end = true;
                    self.video.flush();
                }
            }
        }
    }

    pub fn sequence(&self) -> Option<&Sequence> {
        self.video.sequence()
    }

    pub fn audio(&self) -> Option<AudioInfo> {
        self.audio.info()
    }

    /// Échantillons décodés depuis le dernier appel, entrelacés par voie.
    pub fn take_samples(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.samples)
    }
}

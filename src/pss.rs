//! Conteneur PSS : un flux de programme MPEG (paquets 0xBA), la vidéo en
//! flux 0xE0, le son en flux privé 0xBD. Le son commence chaque paquet par
//! 4 octets de Sony (`ff a0 00 00` sur Dokuro-chan) ; le flux sonore est le
//! quartet bas du deuxième.

use std::io::{self, ErrorKind, Read};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Video(u8),
    Audio(u8),
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub stream: Stream,
    /// Horodatage (90 kHz).
    pub pts: Option<u64>,
    pub data: Vec<u8>,
}

pub struct Demuxer<R> {
    src: R,
}

impl<R: Read> Demuxer<R> {
    pub fn new(src: R) -> Self {
        Self { src }
    }

    /// Paquet suivant de vidéo ou de son ; `None` à la fin.
    pub fn next_packet(&mut self) -> io::Result<Option<Packet>> {
        while let Some(code) = self.start_code()? {
            match code {
                0xBA => {
                    let mut b = [0; 10];
                    if !self.fill(&mut b[..1])? {
                        return Ok(None);
                    }
                    // En-tête de paquet MPEG-2 (01…), sinon MPEG-1 (0010…).
                    let mpeg2 = b[0] >> 6 == 1;
                    if !self.fill(&mut b[1..if mpeg2 { 10 } else { 8 }])? {
                        return Ok(None);
                    }
                    if mpeg2 && !self.skip(usize::from(b[9] & 7))? {
                        return Ok(None);
                    }
                }
                0xB9 => return Ok(None),
                0xBB..=0xFF => {
                    let mut len = [0; 2];
                    if !self.fill(&mut len)? {
                        return Ok(None);
                    }
                    let mut body = vec![0; usize::from(u16::from_be_bytes(len))];
                    if !self.fill(&mut body)? {
                        return Ok(None);
                    }
                    if let Some(p) = packet(code, &body) {
                        return Ok(Some(p));
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }

    /// Octet qui suit le prochain 00 00 01.
    fn start_code(&mut self) -> io::Result<Option<u8>> {
        let mut w = [0; 4];
        if !self.fill(&mut w)? {
            return Ok(None);
        }
        while w[..3] != [0, 0, 1] {
            w.rotate_left(1);
            if !self.fill(&mut w[3..])? {
                return Ok(None);
            }
        }
        Ok(Some(w[3]))
    }

    /// Faux à la fin du fichier.
    fn fill(&mut self, buf: &mut [u8]) -> io::Result<bool> {
        match self.src.read_exact(buf) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn skip(&mut self, n: usize) -> io::Result<bool> {
        self.fill(&mut vec![0; n])
    }
}

fn packet(code: u8, body: &[u8]) -> Option<Packet> {
    let (pts, at) = header(body)?;
    let payload = body.get(at..)?;
    let (stream, data) = match code {
        0xE0..=0xEF => (Stream::Video(code & 0x0F), payload),
        0xBD => (Stream::Audio(payload.get(1)? & 0x0F), payload.get(4..)?),
        _ => return None,
    };
    Some(Packet { stream, pts, data: data.to_vec() })
}

/// En-tête de paquet PES : horodatage et début des données.
fn header(body: &[u8]) -> Option<(Option<u64>, usize)> {
    if body.first()? >> 6 == 2 {
        let pts = (body.get(1)? & 0x80 != 0).then(|| timestamp(body.get(3..8)?)).flatten();
        return Some((pts, 3 + usize::from(*body.get(2)?)));
    }
    // MPEG-1 : bourrage, tampon, horodatages.
    let mut i = body.iter().position(|&b| b != 0xFF)?;
    if body[i] >> 6 == 1 {
        i += 2;
    }
    Some(match body.get(i)? >> 4 {
        2 => (timestamp(body.get(i..i + 5)?), i + 5),
        3 => (timestamp(body.get(i..i + 5)?), i + 10),
        _ => (None, i + 1),
    })
}

fn timestamp(b: &[u8]) -> Option<u64> {
    let v = |i: usize| u64::from(b[i]);
    Some((v(0) >> 1 & 7) << 30 | v(1) << 22 | (v(2) >> 1) << 15 | v(3) << 7 | v(4) >> 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_and_audio_packets() {
        let mut s = vec![0, 0, 1, 0xBA, 0x44, 0, 4, 0, 4, 1, 0, 0xD8, 0x43, 0xF8];
        // Vidéo, horodatage 21 00 01 24 61 (repris d'IF_LOGO.PSS).
        s.extend([0, 0, 1, 0xE0, 0, 10, 0x81, 0x80, 5, 0x21, 0, 1, 0x24, 0x61, 0xAA, 0xBB]);
        s.extend([0, 0, 1, 0xBD, 0, 9, 0x81, 0x00, 0, 0xFF, 0xA0, 0, 0, 1, 2]);
        s.extend([0, 0, 1, 0xB9]);
        let mut d = Demuxer::new(&s[..]);
        let v = d.next_packet().unwrap().unwrap();
        assert_eq!((v.stream, v.pts, &v.data[..]), (Stream::Video(0), Some(0x1230), &[0xAA, 0xBB][..]));
        let a = d.next_packet().unwrap().unwrap();
        assert_eq!((a.stream, a.pts, &a.data[..]), (Stream::Audio(0), None, &[1, 2][..]));
        assert!(d.next_packet().unwrap().is_none());
    }
}

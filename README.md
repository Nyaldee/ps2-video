# ps2-video

English · [Français](README.fr.md) · [日本語](README.jp.md)

PlayStation 2 movie decoding in pure Rust, with no dependencies: PSS container, MPEG-1 and MPEG-2 video, PCM and ADPCM audio. It was written for PC ports of PS2 games and can be reused from one game to the next.

## Supported

- **PSS**: MPEG program stream (MPEG-1 or MPEG-2 packs), with video in stream 0xE0 and audio in private stream 0xBD (Sony `SShd` / `SSbd` headers).
- **MPEG-2 video**, Main Profile 4:2:0 (what the console's IPU decodes):
  - progressive and interlaced pictures;
  - frame pictures and field pictures;
  - frame, field, 16×8 and dual-prime prediction;
  - field DCT;
  - loaded quantiser matrices, alternate scan, non-linear scale, DC precision from 8 to 11 bits.
- **MPEG-1 video**, except D pictures.
- **Audio**: 16-bit PCM and PS2 ADPCM, with channels interleaved in blocks.
- **Frames** in YCbCr 4:2:0, in display order, with their timestamps. Two RGBA conversions (BT.601): `rgba`, like the IPU (each chroma sample repeated over 2 × 2 pixels), and `rgba_smooth`, with chroma interpolated between its samples (sharper colour edges, nothing invented).
- **Display aspect ratio** (`Sequence::display_aspect`): PS2 movie pixels are not square. For example, 640×448 displays as 4:3.

## Usage

```toml
[dependencies]
ps2-video = { git = "https://github.com/Nyaldee/ps2-video" }
```

```rust
let file = std::io::BufReader::new(std::fs::File::open("OP.PSS")?);
let mut movie = ps2_video::Movie::new(file);
while let Some(frame) = movie.frame()? {
    let rgba = frame.rgba();            // frame.width × frame.height
    let samples = movie.take_samples(); // i16, interleaved channels
}
```

`Demuxer`, `Decoder` and `AudioDecoder` can also be used on their own. For example, `Decoder` takes an MPEG elementary stream in chunks (`push`) and returns frames through `frame`.

To see what a movie contains and how fast it decodes:

```
cargo run --release --example pss_info FILE.PSS
```

## Accuracy

The IDCT computes exactly what FFmpeg's "simple" IDCT computes. Every decoded frame can therefore be compared to FFmpeg (`-idct simple`) byte for byte. `cargo test` builds test movies with FFmpeg and compares them:

- progressive with B pictures;
- interlaced (field DCT and prediction, alternate scan, non-linear scale, 10-bit DC);
- odd size with coarse quantisation;
- MPEG-1.

You can also give it real movies to compare, separated as in the PATH:

```
$env:PS2_VIDEO_PSS = "A.PSS;B.PSS"; cargo test --release   # PowerShell
PS2_VIDEO_PSS=A.PSS:B.PSS cargo test --release             # Linux, macOS
```

Checked on all 16 movies of *Game ni Natta yo! Dokuro-chan* (20,133 frames): every frame is bit-identical to FFmpeg.

Without FFmpeg (the `FFMPEG` variable, otherwise `ffmpeg` on the PATH), these tests pass without checking anything.

Field pictures are not covered by these tests, because FFmpeg's encoder does not produce them.

A skipped macroblock in a B picture reuses the previous macroblock's directions, with frame prediction using the predicted vectors. That is how FFmpeg and libmpeg2 read the standard, and this crate does the same.

## Interlacing

Interlaced pictures come out whole, with their two fields woven line by line. Deinterlacing, when a movie needs it, is up to the game's display code. `Frame::progressive` and `Frame::top_field_first` say when it applies.

## License

Copyright (C) 2026 Nyaldee. Licensed under the [GNU General Public License v3.0](LICENSE) — see the `LICENSE` file for the full text.

[Claude](https://claude.com), Anthropic's AI, contributed to the code of this project.

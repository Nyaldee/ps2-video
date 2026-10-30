# ps2-video

[English](README.md) · [Français](README.fr.md) · 日本語

PlayStation 2 のムービーを、依存ライブラリなしの純粋な Rust でデコードするライブラリです。PSS コンテナ、MPEG-1 / MPEG-2 映像、PCM / ADPCM 音声に対応しています。PS2 ゲームの PC 移植のために作られ、ゲームをまたいで再利用できます。

## 対応範囲

- **PSS**：MPEG プログラムストリーム（MPEG-1 / MPEG-2 のパック）。映像はストリーム 0xE0、音声はプライベートストリーム 0xBD（ソニーの `SShd` / `SSbd` ヘッダー）。
- **MPEG-2 映像**：メインプロファイル 4:2:0（本体の IPU がデコードする形式）
  - プログレッシブ / インターレースのピクチャ
  - フレームピクチャ / フィールドピクチャ
  - フレーム予測、フィールド予測、16×8 予測、デュアルプライム予測
  - フィールド DCT
  - 量子化マトリクスの読み込み、オルタネートスキャン、非線形量子化スケール、DC 精度 8〜11 ビット
- **MPEG-1 映像**（D ピクチャを除く）
- **音声**：16 ビット PCM と PS2 ADPCM（チャンネルはブロック単位でインターリーブ）
- **フレーム**：YCbCr 4:2:0 で、表示順に、タイムスタンプとともに出力されます。RGBA 変換（BT.601）は 2 種類あります。`rgba` は IPU と同じく色差サンプルを 2 × 2 画素に繰り返し、`rgba_smooth` は色差をサンプル間で補間します（色の境界がよりシャープになり、何も付け加えません）。
- **表示アスペクト比**（`Sequence::display_aspect`）：PS2 ムービーの画素は正方形ではありません。たとえば 640×448 は 4:3 で表示されます。

## 使い方

```toml
[dependencies]
ps2-video = { git = "https://github.com/Nyaldee/ps2-video" }
```

```rust
let file = std::io::BufReader::new(std::fs::File::open("OP.PSS")?);
let mut movie = ps2_video::Movie::new(file);
while let Some(frame) = movie.frame()? {
    let rgba = frame.rgba();            // frame.width × frame.height
    let samples = movie.take_samples(); // i16、チャンネルはインターリーブ
}
```

`Demuxer`、`Decoder`、`AudioDecoder` は単独でも使えます。たとえば `Decoder` は MPEG エレメンタリストリームを少しずつ受け取り（`push`）、`frame` でフレームを返します。

ムービーの中身とデコード速度を確認するには：

```
cargo run --release --example pss_info FILE.PSS
```

## 正確さ

IDCT は FFmpeg の「simple」IDCT とまったく同じ計算をします。そのため、デコードした各フレームを FFmpeg（`-idct simple`）と 1 バイト単位で比較できます。`cargo test` は FFmpeg でテスト用ムービーを作り、次のものを比較します。

- B ピクチャを含むプログレッシブ映像
- インターレース映像（フィールド DCT とフィールド予測、オルタネートスキャン、非線形量子化スケール、10 ビット DC）
- 粗い量子化の奇数サイズ映像
- MPEG-1

実際のムービーを PATH と同じ区切り文字で指定して比較することもできます。

```
$env:PS2_VIDEO_PSS = "A.PSS;B.PSS"; cargo test --release   # PowerShell
PS2_VIDEO_PSS=A.PSS:B.PSS cargo test --release             # Linux、macOS
```

『ゲームになったよ！ドクロちゃん』の全 16 ムービー（20,133 フレーム）で確認済みで、すべてのフレームが FFmpeg とビット単位で一致します。

FFmpeg がない場合（`FFMPEG` 変数、なければ PATH 上の `ffmpeg`）、これらのテストは何も確認せずに成功します。

FFmpeg のエンコーダーはフィールドピクチャを出力しないため、フィールドピクチャはこれらのテストの対象外です。

B ピクチャのスキップされたマクロブロックは、直前のマクロブロックの予測方向を引き継ぎ、予測ベクトルを使ったフレーム予測になります。FFmpeg と libmpeg2 による規格の解釈であり、このライブラリも同じ解釈に従います。

## インターレース

インターレースのピクチャは、2 つのフィールドを 1 行ずつ組み合わせたフレームとして出力されます。デインターレースが必要な場合は、ゲーム側の表示処理で行います。`Frame::progressive` と `Frame::top_field_first` でその判断ができます。

## ライセンス

Copyright (C) 2026 Nyaldee. [GNU General Public License v3.0](LICENSE) のもとで配布されています。全文は `LICENSE` ファイルを参照してください。

AnthropicのAIである[Claude](https://claude.com)が、このプロジェクトのコードに貢献しています。

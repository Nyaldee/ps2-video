# ps2-video

[English](README.md) · Français · [日本語](README.jp.md)

Décodage des vidéos de la PlayStation 2, en Rust pur et sans dépendance : conteneur PSS, vidéo MPEG-1 et MPEG-2, son PCM et ADPCM. Écrit pour les portages PC de jeux PS2, réutilisable d'un jeu à l'autre.

## Ce qui est pris en charge

- **PSS** : flux de programme MPEG (paquets MPEG-1 ou MPEG-2), vidéo en flux 0xE0, son en flux privé 0xBD (en-têtes `SShd` / `SSbd` de Sony).
- **Vidéo MPEG-2**, profil principal en 4:2:0 (celui que décode l'IPU de la console) :
  - images progressives et entrelacées ;
  - images entières et images en deux trames ;
  - prédiction d'image, de trame, 16×8 et « dual prime » ;
  - DCT de trame ;
  - matrices de quantification chargées, parcours alterné, échelle non linéaire, précision du continu de 8 à 11 bits.
- **Vidéo MPEG-1**, sauf les images D.
- **Son** : PCM 16 bits et ADPCM de la PS2, voies entrelacées par blocs.
- **Images** en YCbCr 4:2:0, dans l'ordre d'affichage, avec leur horodatage. Deux conversions RGBA (BT.601) : `rgba`, comme l'IPU (chaque échantillon de couleur répété sur 2 × 2 pixels), et `rgba_smooth`, la couleur interpolée entre ses échantillons (des bords de couleur plus nets, sans rien inventer).
- **Proportions d'affichage** (`Sequence::display_aspect`) : les pixels des vidéos PS2 ne sont pas carrés. Par exemple, 640×448 s'affiche en 4:3.

## Utilisation

```toml
[dependencies]
ps2-video = { git = "https://github.com/Nyaldee/ps2-video" }
```

```rust
let file = std::io::BufReader::new(std::fs::File::open("OP.PSS")?);
let mut movie = ps2_video::Movie::new(file);
while let Some(frame) = movie.frame()? {
    let rgba = frame.rgba();            // frame.width × frame.height
    let samples = movie.take_samples(); // i16, voies entrelacées
}
```

`Demuxer`, `Decoder` et `AudioDecoder` s'utilisent aussi séparément. Par exemple, `Decoder` reçoit un flux MPEG élémentaire par morceaux (`push`) et rend les images par `frame`.

Pour voir ce que contient une vidéo et à quelle vitesse elle se décode :

```
cargo run --release --example pss_info FICHIER.PSS
```

## Exactitude

L'IDCT fait le même calcul que l'IDCT « simple » de FFmpeg. On peut donc comparer chaque image décodée à FFmpeg (`-idct simple`), octet par octet. `cargo test` fabrique des vidéos de test avec FFmpeg et les compare :

- progressive avec images B ;
- entrelacée (DCT et prédiction de trame, parcours alterné, échelle non linéaire, continu sur 10 bits) ;
- taille impaire à forte quantification ;
- MPEG-1.

On peut aussi lui donner de vraies vidéos à comparer, séparées comme dans le PATH :

```
$env:PS2_VIDEO_PSS = "A.PSS;B.PSS"; cargo test --release   # PowerShell
PS2_VIDEO_PSS=A.PSS:B.PSS cargo test --release             # Linux, macOS
```

Vérifié sur les 16 vidéos de *Game ni Natta yo! Dokuro-chan* (20 133 images) : toutes sont identiques à FFmpeg au bit près.

Sans FFmpeg (variable `FFMPEG`, sinon `ffmpeg` du PATH), ces tests passent sans rien vérifier.

Les images en deux trames ne sont pas couvertes par ces tests, parce que l'encodeur de FFmpeg n'en produit pas.

Un macrobloc sauté d'une image B reprend les directions du précédent, en prédiction d'image aux vecteurs prédits. C'est la lecture de FFmpeg et de libmpeg2, et c'est celle que suit cette bibliothèque.

## Entrelacé

Les images entrelacées sont rendues entières, avec leurs deux trames mêlées une ligne sur deux. Les désentrelacer, si une vidéo en a besoin, revient à l'affichage du jeu. `Frame::progressive` et `Frame::top_field_first` le disent.

## Licence

Copyright (C) 2026 Nyaldee. Distribué sous licence [GNU General Public License v3.0](LICENSE) — voir le fichier `LICENSE` pour le texte complet.

[Claude](https://claude.com), l'IA d'Anthropic, a contribué au code de ce projet.

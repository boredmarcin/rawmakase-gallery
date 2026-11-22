# RAWmakase Web Gallery

A [RAWmakase](https://github.com/pch/rawmakase) extension that turns an export into a web gallery: a full-screen cover, a grid of the photos, a full-screen viewer with a slideshow, and favorites a client can send back as a list of file names.

The gallery is plain HTML, CSS and JavaScript in a `gallery` folder beside the exported photos. It opens from disk or from any web host, needs no server and loads nothing from elsewhere (Montserrat is included, under the SIL Open Font License).

## Use

1. Install the `.rmext` from Preferences > Extensions > Add Extension > From File….
2. In Export…, choose Send To > Web Gallery and fill in its options:
   - **Title**: empty uses the export folder's name.
   - **Date**: empty uses when the photos were taken.
   - **Author** and **Contact e-mail**: on the cover, and where favorites are sent.
   - **Layout**: Masonry, Grid or Column.
3. Export JPEGs. The photo with the yellow label is the cover; without one, the first photo is.

Needs RAWmakase with export destinations (extension api 4).

## Build

```
cargo rawmakase build
cargo rawmakase package
```

`rawmakase-gallery-cli <folder> -o <out> --title … --layout masonry|grid|column` builds a gallery from a folder of JPEGs without RAWmakase.

## License

MIT. The bundled Montserrat font is under the SIL Open Font License 1.1 (`assets/fonts/OFL.txt`).

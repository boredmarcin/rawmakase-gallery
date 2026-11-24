//! Builds a static web gallery (HTML, CSS, JS and resized JPEGs) from a set of
//! JPEG files. The page needs no server: its data is inlined, so it opens from
//! disk as well as from any web host. [`extension`] is the RAWmakase
//! extension that builds one from the photos an export published.

use anyhow::{Context as _, Result, bail, ensure};
use image::{
    DynamicImage, ImageDecoder, ImageReader, codecs::jpeg::JpegEncoder, imageops::FilterType,
};
use rawmakase_extension::protocol::method::{
    ExportBegin, ExportEnd, ExportEnded, ExportFile, FileResult, Sent,
};
use rawmakase_extension::{
    Context, Delivery, ErrorCode, ExportDestination, Extension, RemoteError, collected,
};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
};

/// The longest edge of the photos the viewer shows and downloads.
pub const LARGE_EDGE: u32 = 2400;
/// The width of the grid's thumbnails.
pub const THUMB_WIDTH: u32 = 800;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// Columns of photos at their own proportions, filled left to right.
    Masonry,
    /// An even grid of square crops.
    Grid,
    /// One photo per row, at its own proportions.
    Column,
}

impl std::str::FromStr for Layout {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "masonry" => Self::Masonry,
            "grid" => Self::Grid,
            "column" => Self::Column,
            _ => bail!("unknown layout {s:?}: masonry, grid or column"),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub title: String,
    /// Shown under the title, such as the date of the shoot.
    pub subtitle: String,
    /// The photographer, in the footer.
    pub author: String,
    /// Where a client's favourites go, offered as an e-mail link.
    pub email: String,
    pub layout: Layout,
    /// The photo on the cover, by its place in `photos`; the first by default.
    pub cover: usize,
    /// Each photo's id, by its place in `photos`, which a client's favorites
    /// keep it by: the catalog's UUID from RAWmakase. A photo without one is
    /// kept by its file name.
    pub ids: Vec<Option<String>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            title: "Gallery".into(),
            subtitle: String::new(),
            author: String::new(),
            email: String::new(),
            layout: Layout::Masonry,
            cover: 0,
            ids: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Photo {
    /// The file name in `photos/large` and `photos/thumb`.
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Its id for favorites ([`Options::ids`]); none keeps it by `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Serialize)]
struct Data<'a> {
    id: String,
    title: &'a str,
    subtitle: &'a str,
    author: &'a str,
    email: &'a str,
    layout: Layout,
    cover: usize,
    photos: &'a [Photo],
}

/// Writes the gallery of `photos` into `out`, which it creates. Returns the
/// photos as the page lists them.
pub fn build(photos: &[PathBuf], out: &Path, options: &Options) -> Result<Vec<Photo>> {
    ensure!(!photos.is_empty(), "no photos to put in the gallery");
    ensure!(
        options.cover < photos.len(),
        "the cover photo is not among the photos"
    );
    let large = out.join("photos/large");
    let thumb = out.join("photos/thumb");
    fs::create_dir_all(&large).with_context(|| format!("creating {}", large.display()))?;
    fs::create_dir_all(&thumb)?;
    let names = unique_names(photos);
    let jobs: Vec<(&PathBuf, &String)> = photos.iter().zip(&names).collect();
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(jobs.len());
    let chunk = jobs.len().div_ceil(threads);
    let results: Vec<Result<Photo>> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .chunks(chunk)
            .map(|part| {
                let (large, thumb) = (&large, &thumb);
                scope.spawn(move || {
                    part.iter()
                        .map(|(path, name)| {
                            resize(path, name, large, thumb)
                                .with_context(|| format!("{}", path.display()))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a resize thread panicked"))
            .collect()
    });
    let mut photos = results.into_iter().collect::<Result<Vec<_>>>()?;
    for (photo, id) in photos.iter_mut().zip(&options.ids) {
        photo.id.clone_from(id);
    }
    let data = Data {
        id: gallery_id(&options.title, &photos),
        title: &options.title,
        subtitle: &options.subtitle,
        author: &options.author,
        email: &options.email,
        layout: options.layout,
        cover: options.cover,
        photos: &photos,
    };
    fs::create_dir_all(out.join("assets"))?;
    fs::write(
        out.join("assets/gallery.css"),
        include_str!("../assets/gallery.css"),
    )?;
    fs::write(
        out.join("assets/gallery.js"),
        include_str!("../assets/gallery.js"),
    )?;
    // Montserrat (SIL Open Font License, its text beside the fonts), served with
    // the gallery so it needs no font host.
    fs::create_dir_all(out.join("assets/fonts"))?;
    for (name, bytes) in [
        (
            "montserrat-latin.woff2",
            &include_bytes!("../assets/fonts/montserrat-latin.woff2")[..],
        ),
        (
            "montserrat-latin-ext.woff2",
            &include_bytes!("../assets/fonts/montserrat-latin-ext.woff2")[..],
        ),
        ("OFL.txt", &include_bytes!("../assets/fonts/OFL.txt")[..]),
    ] {
        fs::write(out.join("assets/fonts").join(name), bytes)?;
    }
    fs::write(out.join("index.html"), page(&data)?)?;
    Ok(photos)
}

/// Every JPEG directly in `dir`, by name.
pub fn jpegs_in(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg")
                })
        })
        .collect();
    found.sort();
    Ok(found)
}

fn resize(path: &Path, name: &str, large: &Path, thumb: &Path) -> Result<Photo> {
    let mut decoder = ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let (width, height) = (image.width(), image.height());
    let upright = orientation == image::metadata::Orientation::NoTransforms;
    let shown = if width.max(height) > LARGE_EDGE {
        let fitted = image.resize(LARGE_EDGE, LARGE_EDGE, FilterType::Lanczos3);
        save(&fitted, &large.join(name), 90)?;
        fitted
    } else if upright {
        // Already small enough: the exported file as it is, metadata and all.
        fs::copy(path, large.join(name))?;
        image
    } else {
        save(&image, &large.join(name), 92)?;
        image
    };
    let small = if shown.width() > THUMB_WIDTH {
        shown.resize(THUMB_WIDTH, u32::MAX, FilterType::CatmullRom)
    } else {
        shown.clone()
    };
    save(&small, &thumb.join(name), 82)?;
    Ok(Photo {
        name: name.to_string(),
        width: shown.width(),
        height: shown.height(),
        id: None,
    })
}

fn save(image: &DynamicImage, to: &Path, quality: u8) -> Result<()> {
    let file = BufWriter::new(fs::File::create(to)?);
    image
        .to_rgb8()
        .write_with_encoder(JpegEncoder::new_with_quality(file, quality))?;
    Ok(())
}

/// File names safe in a URL, each once: `a b.JPG` is `a-b.jpg`, a second
/// `a-b.jpg` is `a-b-2.jpg`.
fn unique_names(photos: &[PathBuf]) -> Vec<String> {
    let mut taken = HashSet::new();
    photos
        .iter()
        .map(|p| {
            let stem: String = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("photo")
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            let stem = if stem.trim_matches('-').is_empty() {
                "photo".to_string()
            } else {
                stem
            };
            let mut name = format!("{stem}.jpg");
            let mut n = 2;
            while !taken.insert(name.to_ascii_lowercase()) {
                name = format!("{stem}-{n}.jpg");
                n += 1;
            }
            name
        })
        .collect()
}

/// Keeps one gallery's favourites apart from another's in the browser.
fn gallery_id(title: &str, photos: &[Photo]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in title
        .bytes()
        .chain(photos.iter().flat_map(|p| p.name.bytes()))
    {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn page(data: &Data) -> Result<String> {
    // `</` can't end the script early once escaped.
    let json = serde_json::to_string(data)?.replace("</", "<\\/");
    let cover = &data.photos[data.cover];
    Ok(include_str!("../assets/index.html")
        .replace("{{TITLE}}", &escape(data.title))
        .replace("{{COVER}}", &escape(&cover.name))
        .replace("{{DATA}}", &json))
}

/// A cover line for photos taken on `dates` (each starting `YYYY-MM-DD`, as
/// EXIF and ISO 8601 dates do): "July 15, 2022" for one day, "July 2022" for
/// one month, "July - August 2022" or "December 2021 - January 2022" across
/// months. Empty when no date reads.
pub fn date_line(dates: &[&str]) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let mut days: Vec<(u32, usize, u32)> = dates
        .iter()
        .filter_map(|d| {
            let d = d.get(..10)?.replace(':', "-");
            let mut parts = d.split('-');
            let year = parts.next()?.parse().ok()?;
            let month: usize = parts.next()?.parse().ok()?;
            let day = parts.next()?.parse().ok()?;
            (1..=12).contains(&month).then_some((year, month, day))
        })
        .collect();
    days.sort_unstable();
    let (Some(&(y0, m0, d0)), Some(&(y1, m1, _))) = (days.first(), days.last()) else {
        return String::new();
    };
    let (first, last) = (MONTHS[m0 - 1], MONTHS[m1 - 1]);
    if days.first() == days.last() {
        format!("{first} {d0}, {y0}")
    } else if (y0, m0) == (y1, m1) {
        format!("{first} {y0}")
    } else if y0 == y1 {
        format!("{first} - {last} {y0}")
    } else {
        format!("{first} {y0} - {last} {y1}")
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The gallery's folder, inside the export's folder.
pub const FOLDER: &str = "gallery";

/// Builds the gallery from a whole delivery, its files in the export's
/// order, in a [`FOLDER`] folder inside the export's folder.
fn deliver(cx: &Context, mut delivery: Delivery) -> Result<ExportEnded, RemoteError> {
    let folder = delivery.begin.folder.clone().ok_or_else(scattered)?;
    let mut files = std::mem::take(&mut delivery.files);
    files.sort_by_key(|f| f.index);
    if files.is_empty() {
        return Ok(ExportEnded::default());
    }
    cx.progress(None, Some("Building the gallery"));
    let begin = &delivery.begin;
    let photos: Vec<_> = files.iter().map(|f| f.path.clone()).collect();
    let dates: Vec<&str> = files.iter().filter_map(|f| f.captured.as_deref()).collect();
    // An option left empty keeps what the export gives: the folder's name, the dates.
    let text = |id: &str| {
        begin
            .option_str(id)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let layout = match begin.option_str("layout") {
        Some(layout) => layout
            .to_ascii_lowercase()
            .parse()
            .map_err(|e| RemoteError::new(ErrorCode::InvalidRequest, format!("{e:#}")))?,
        None => Layout::Masonry,
    };
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let options = Options {
        title: text("title").unwrap_or_else(|| name.clone()),
        subtitle: text("subtitle").unwrap_or_else(|| date_line(&dates)),
        author: text("author").unwrap_or_default(),
        email: text("email").unwrap_or_default(),
        layout,
        cover: cover(&files),
        ids: files.iter().map(|f| f.photo_uuid.clone()).collect(),
    };
    let out = folder.join(FOLDER);
    build(&photos, &out, &options)
        .map_err(|e| RemoteError::new(ErrorCode::Unavailable, format!("{e:#}")))?;
    Ok(ExportEnded {
        files: files
            .iter()
            .map(|f| FileResult {
                index: f.index,
                result: Sent::Sent,
                message: None,
                remote_id: None,
                url: None,
            })
            .collect(),
        message: Some(format!("Built the gallery {}", options.title)),
        location: Some(out.join("index.html").display().to_string()),
    })
}

/// Why an export beside its originals has no gallery.
fn scattered() -> RemoteError {
    RemoteError::new(
        ErrorCode::InvalidRequest,
        "the photos go to several folders; export them to one",
    )
}

/// The cover: the first photo with the yellow label, or else the first.
fn cover(files: &[ExportFile]) -> usize {
    files
        .iter()
        .position(|f| {
            f.catalog
                .as_ref()
                .is_some_and(|c| c.label.as_deref() == Some("yellow"))
        })
        .unwrap_or(0)
}

/// The `web` destination: the whole delivery is collected and built at its
/// end, and an export with no one folder is refused as it begins.
struct Gallery<T>(T);
impl<T: ExportDestination> ExportDestination for Gallery<T> {
    fn begin(&mut self, cx: &Context, begin: ExportBegin) -> Result<(), RemoteError> {
        if begin.folder.is_none() {
            return Err(scattered());
        }
        self.0.begin(cx, begin)
    }
    fn file(&mut self, cx: &Context, file: ExportFile) -> Result<(), RemoteError> {
        self.0.file(cx, file)
    }
    fn end(&mut self, cx: &Context, end: ExportEnd) -> Result<ExportEnded, RemoteError> {
        self.0.end(cx, end)
    }
}

/// The extension: its `web` export destination builds the gallery in a
/// [`FOLDER`] folder beside the exported photos.
pub fn extension() -> Extension {
    Extension::new().export_destination("web", Gallery(collected(deliver)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let path = dir.join(name);
        save(&DynamicImage::new_rgb8(w, h), &path, 80).unwrap();
        path
    }

    #[test]
    fn builds_a_gallery_with_resized_photos_and_safe_names() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let photos = vec![
            jpeg(src.path(), "big one.jpg", 3000, 2000),
            jpeg(src.path(), "small.jpg", 600, 900),
            jpeg(&src.path().join("."), "Big-One.JPG", 10, 10),
        ];
        let options = Options {
            title: "Ania & Tomek </script>".into(),
            ..Options::default()
        };
        let built = build(&photos, out.path(), &options).unwrap();
        assert_eq!(
            built[0],
            Photo {
                name: "big-one.jpg".into(),
                width: 2400,
                height: 1600,
                id: None
            }
        );
        assert_eq!(
            built[1],
            Photo {
                name: "small.jpg".into(),
                width: 600,
                height: 900,
                id: None
            }
        );
        assert_eq!(built[2].name, "Big-One-2.jpg");
        let thumb = image::image_dimensions(out.path().join("photos/thumb/big-one.jpg")).unwrap();
        assert_eq!(thumb, (800, 533));
        let html = fs::read_to_string(out.path().join("index.html")).unwrap();
        assert!(html.contains("<title>Ania &amp; Tomek &lt;/script&gt;</title>"));
        assert!(
            !html.contains("Tomek </script>"),
            "the title can't end the data script"
        );
        assert!(out.path().join("assets/gallery.js").exists());
        assert!(
            out.path()
                .join("assets/fonts/montserrat-latin-ext.woff2")
                .exists()
        );
    }

    #[test]
    fn the_date_line_spans_the_photos_dates() {
        assert_eq!(
            date_line(&["2022-07-15T10:00:00", "2022:07:15 18:30:00"]),
            "July 15, 2022"
        );
        assert_eq!(date_line(&["2022-07-20", "2022-07-15"]), "July 2022");
        assert_eq!(
            date_line(&["2022-07-31", "2022-08-01"]),
            "July - August 2022"
        );
        assert_eq!(
            date_line(&["2021-12-31", "2022-01-01"]),
            "December 2021 - January 2022"
        );
        assert_eq!(date_line(&["", "nonsense", "2022-13-01"]), "");
        assert_eq!(date_line(&[]), "");
    }

    #[test]
    fn refuses_no_photos_and_a_cover_out_of_range() {
        let out = tempfile::tempdir().unwrap();
        assert!(build(&[], out.path(), &Options::default()).is_err());
        let src = tempfile::tempdir().unwrap();
        let one = vec![jpeg(src.path(), "a.jpg", 4, 4)];
        let options = Options {
            cover: 1,
            ..Options::default()
        };
        assert!(build(&one, out.path(), &options).is_err());
    }
}

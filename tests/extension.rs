//! The extension as RAWmakase drives it: a delivery builds the gallery.
use image::{DynamicImage, codecs::jpeg::JpegEncoder};
use rawmakase_extension::ServeOptions;
use rawmakase_extension::mock::MockHost;
use rawmakase_extension::protocol::method::{AppRequest, Delivered, ExportDeliver, ExportedFile};
use serde_json::Value;
use std::path::Path;

fn jpeg(path: &Path, w: u32, h: u32) {
    let file = std::fs::File::create(path).unwrap();
    DynamicImage::new_rgb8(w, h)
        .to_rgb8()
        .write_with_encoder(JpegEncoder::new_with_quality(file, 80))
        .unwrap();
}

#[test]
fn a_delivery_builds_the_gallery_beside_the_photos() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Chorwacja 2022");
    std::fs::create_dir(&folder).unwrap();
    let files: Vec<ExportedFile> = [
        ("a.jpg", "2022-07-15T10:00:00"),
        ("b.jpg", "2022-07-15T18:00:00"),
    ]
    .iter()
    .map(|(name, captured)| {
        let path = folder.join(name);
        jpeg(&path, 300, 200);
        ExportedFile {
            path,
            name: (*name).into(),
            width: Some(300),
            height: Some(200),
            captured: Some((*captured).into()),
            // The second photo, labelled yellow, is the cover.
            label: (*name == "b.jpg").then(|| "yellow".into()),
            rating: Some(0),
            flag: None,
        }
    })
    .collect();
    let host = MockHost::start_with_options(
        rawmakase_gallery::extension(),
        ServeOptions { api: 4 },
        |_, _| Ok(Value::Null),
    );
    assert_eq!(host.hello().unwrap().implements, ["export-destination/web"]);
    let answer = host
        .request(AppRequest::ExportDeliver(ExportDeliver {
            destination: "web".into(),
            folder: folder.clone(),
            label: "Chorwacja 2022".into(),
            options: serde_json::from_value(serde_json::json!({
                "title": "", "subtitle": "", "author": "Marcin", "email": "", "layout": "Grid"
            }))
            .unwrap(),
            files,
        }))
        .unwrap();
    let delivered: Delivered = serde_json::from_value(answer).unwrap();
    assert_eq!(delivered.sent, 2);
    let index = folder.join(rawmakase_gallery::FOLDER).join("index.html");
    assert_eq!(delivered.location.as_deref(), Some(index.to_str().unwrap()));
    let html = std::fs::read_to_string(&index).unwrap();
    assert!(html.contains("<title>Chorwacja 2022</title>"));
    assert!(
        html.contains("July 15, 2022"),
        "the cover's date comes from the photos"
    );
    assert!(folder.join("gallery/photos/thumb/b.jpg").exists());
    assert!(
        html.contains(r#"src="photos/large/b.jpg""#),
        "the yellow photo is the cover"
    );
    assert!(html.contains(r#""author":"Marcin""#) && html.contains(r#""layout":"grid""#));
    host.finish().unwrap();
}

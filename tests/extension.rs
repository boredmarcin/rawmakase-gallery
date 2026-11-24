//! The extension as RAWmakase drives it: a delivery builds the gallery.
use image::{DynamicImage, codecs::jpeg::JpegEncoder};
use rawmakase_extension::ServeOptions;
use rawmakase_extension::mock::MockHost;
use rawmakase_extension::protocol::ErrorCode;
use rawmakase_extension::protocol::method::{
    AppRequest, CatalogFacts, ExportBegin, ExportEnd, ExportEnded, ExportFile,
};
use serde_json::Value;
use std::path::Path;

fn jpeg(path: &Path, w: u32, h: u32) {
    let file = std::fs::File::create(path).unwrap();
    DynamicImage::new_rgb8(w, h)
        .to_rgb8()
        .write_with_encoder(JpegEncoder::new_with_quality(file, 80))
        .unwrap();
}

fn begin(folder: Option<&Path>) -> ExportBegin {
    ExportBegin {
        delivery: "1".into(),
        destination: "web".into(),
        folder: folder.map(Path::to_path_buf),
        count: 2,
        options: serde_json::from_value(serde_json::json!({
            "title": "", "subtitle": "", "author": "Marcin", "email": "", "layout": "Grid"
        }))
        .unwrap(),
    }
}

#[test]
fn a_delivery_builds_the_gallery_beside_the_photos() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Chorwacja 2022");
    std::fs::create_dir(&folder).unwrap();
    // Written side by side, the second photo comes first.
    let files: Vec<ExportFile> = [
        (
            1,
            "b.jpg",
            "2022-07-15T18:00:00",
            Some("6f1c2d3e-0000-4000-8000-000000000002"),
        ),
        (0, "a.jpg", "2022-07-15T10:00:00", None),
    ]
    .iter()
    .map(|(index, name, captured, uuid)| {
        let path = folder.join(name);
        jpeg(&path, 300, 200);
        ExportFile {
            delivery: "1".into(),
            destination: "web".into(),
            index: *index,
            path,
            name: (*name).into(),
            width: Some(300),
            height: Some(200),
            captured: Some((*captured).into()),
            photo_uuid: uuid.map(str::to_string),
            // The second photo, labelled yellow, is the cover.
            catalog: Some(CatalogFacts {
                label: (*name == "b.jpg").then(|| "yellow".into()),
                ..Default::default()
            }),
        }
    })
    .collect();
    let host = MockHost::start_with_options(
        rawmakase_gallery::extension(),
        ServeOptions { api: 4 },
        |_, _| Ok(Value::Null),
    );
    assert_eq!(host.hello().unwrap().implements, ["export-destination/web"]);
    host.request(AppRequest::ExportBegin(begin(Some(&folder))))
        .unwrap();
    for file in files {
        host.request(AppRequest::ExportFile(file)).unwrap();
    }
    let answer = host
        .request(AppRequest::ExportEnd(ExportEnd {
            delivery: "1".into(),
            destination: "web".into(),
            files: 2,
            cancelled: false,
        }))
        .unwrap();
    let ended: ExportEnded = serde_json::from_value(answer).unwrap();
    // Both went, so neither needs a result.
    assert!(ended.files.is_empty());
    let index = folder.join(rawmakase_gallery::FOLDER).join("index.html");
    assert_eq!(ended.location.as_deref(), Some(index.to_str().unwrap()));
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
    // In the export's order, favorites kept by the UUID where there is one.
    let a = html.find(r#""name":"a.jpg""#).unwrap();
    let b = html.find(r#""name":"b.jpg""#).unwrap();
    assert!(a < b);
    assert!(html.contains(r#""id":"6f1c2d3e-0000-4000-8000-000000000002""#));
    assert_eq!(
        html.matches(r#""id":"#).count(),
        2,
        "the gallery's own id and b's"
    );

    // Beside the originals there is no one folder: refused as it begins.
    let error = host
        .request(AppRequest::ExportBegin(begin(None)))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    host.finish().unwrap();
}

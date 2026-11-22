//! The extension program: RAWmakase starts it to build a web gallery from
//! the photos an export published.

fn main() -> std::io::Result<()> {
    rawmakase_gallery::extension().run()
}

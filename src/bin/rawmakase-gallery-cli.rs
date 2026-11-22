use anyhow::{Context, Result, bail};
use rawmakase_gallery::{Options, build, jpegs_in};
use std::path::PathBuf;

const USAGE: &str = "usage: rawmakase-gallery <folder or JPEGs…> -o <out> [--title T] [--subtitle S] \
[--author A] [--email E] [--layout masonry|grid|column] [--cover N]";

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut inputs = Vec::new();
    let mut out = None;
    let mut options = Options::default();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .with_context(|| format!("{arg} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "-o" | "--out" => out = Some(PathBuf::from(value()?)),
            "--title" => options.title = value()?,
            "--subtitle" => options.subtitle = value()?,
            "--author" => options.author = value()?,
            "--email" => options.email = value()?,
            "--layout" => options.layout = value()?.parse()?,
            "--cover" => options.cover = value()?.parse::<usize>()?.saturating_sub(1),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if arg.starts_with('-') => bail!("unknown option {arg}\n{USAGE}"),
            _ => inputs.push(PathBuf::from(arg)),
        }
    }
    let Some(out) = out else { bail!("{USAGE}") };
    let mut photos = Vec::new();
    for input in inputs {
        if input.is_dir() {
            photos.extend(jpegs_in(&input)?);
        } else {
            photos.push(input);
        }
    }
    let built = build(&photos, &out, &options)?;
    println!(
        "{} photos -> {}",
        built.len(),
        out.join("index.html").display()
    );
    Ok(())
}

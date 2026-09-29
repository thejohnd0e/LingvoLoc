use std::{env, error::Error, path::PathBuf};

use image::ImageFormat;
use pdfium_render::prelude::*;

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: pdf-feasibility <pdf>")?;
    let pages = pdf_extract::extract_text_by_pages(&path)?;

    println!("pages={}", pages.len());
    for (index, page) in pages.iter().enumerate() {
        let normalized = page.split_whitespace().collect::<Vec<_>>().join(" ");
        println!(
            "page={} chars={} text={}",
            index + 1,
            page.chars().count(),
            normalized
        );
    }

    let bindings = match Pdfium::bind_to_system_library()
        .or_else(|_| Pdfium::bind_to_library("tools/pdf-feasibility/bin/pdfium.dll"))
    {
        Ok(bindings) => bindings,
        Err(error) => {
            println!("pdfium_available=false error={error}");
            return Ok(());
        }
    };
    let pdfium = Pdfium::new(bindings);
    let document = pdfium.load_pdf_from_file(&path, None)?;
    let render_config = PdfRenderConfig::new().set_target_width(1200);
    for (index, page) in document.pages().iter().enumerate() {
        let text = page.text()?;
        let bounds = text
            .chars()
            .iter()
            .filter_map(|character| character.loose_bounds().ok())
            .fold(None::<(f32, f32, f32, f32)>, |bounds, rect| {
                Some(match bounds {
                    Some((left, bottom, right, top)) => (
                        left.min(rect.left().value),
                        bottom.min(rect.bottom().value),
                        right.max(rect.right().value),
                        top.max(rect.top().value),
                    ),
                    None => (
                        rect.left().value,
                        rect.bottom().value,
                        rect.right().value,
                        rect.top().value,
                    ),
                })
            });
        println!(
            "pdfium_page={} text_chars={} bounds={bounds:?} rendered=true",
            index + 1,
            text.all().chars().count()
        );
        page.render_with_config(&render_config)?
            .as_image()?
            .save_with_format(
                format!("{}-pdfium-page-{}.png", path.display(), index + 1),
                ImageFormat::Png,
            )?;
    }

    Ok(())
}

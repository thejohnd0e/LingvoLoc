use pdfium_render::prelude::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pdfium = Pdfium::new(Pdfium::bind_to_library("bin/pdfium.dll").unwrap());
    let doc = pdfium.load_pdf_from_file(&a[1], None).unwrap();
    for n in &a[3..] {
        let i: i32 = n.parse().unwrap();
        let p = doc.pages().get(i as PdfPageIndex).unwrap();
        p.render_with_config(&PdfRenderConfig::new().set_target_width(900)).unwrap().as_image().unwrap().save(format!("{}-{i}.png", a[2])).unwrap();
    }
}

use pdfium_render::prelude::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pdfium = Pdfium::new(Pdfium::bind_to_library("bin/pdfium.dll").unwrap());
    let doc = pdfium.load_pdf_from_file(&a[1], None).unwrap();
    let page: usize = a[2].parse().unwrap();
    let p = doc.pages().get(page as i32 as PdfPageIndex).unwrap();
    println!("size {}x{}", p.width().value, p.height().value);
    for (i, o) in p.objects().iter().enumerate() {
        let b = o.bounds().unwrap();
        let kind = match o.object_type() { PdfPageObjectType::Text => "T", PdfPageObjectType::Path => "P", PdfPageObjectType::Image => "I", _ => "?" };
        let f = o.matrix().map(|m| m.f()).unwrap_or(-1.0);
        let extra = if let Some(t) = o.as_text_object() {
            format!("f={f:.1} {} sz={:.1} {:?}", t.font().name(), t.scaled_font_size().value, t.text().chars().take(60).collect::<String>())
        } else { String::new() };
        println!("{i:3} {kind} [{:.1} {:.1} {:.1} {:.1}] {extra}", b.left().value, b.bottom().value, b.right().value, b.top().value);
    }
}

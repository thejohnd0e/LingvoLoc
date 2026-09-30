use pdfium_render::prelude::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pdfium = Pdfium::new(Pdfium::bind_to_library("bin/pdfium.dll").unwrap());
    let doc = pdfium.load_pdf_from_file(&a[1], None).unwrap();
    let p = doc.pages().get(a[2].parse::<i32>().unwrap() as PdfPageIndex).unwrap();
    println!("{}", p.text().unwrap().all());
}

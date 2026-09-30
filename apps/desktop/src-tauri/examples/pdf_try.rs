//! Developer probe: analyze a PDF, fake-translate it (Cyrillic transliteration,
//! ~25% longer) and export, to inspect layout preservation without a model.
use lingvoloc_lib::documents::pdf;

fn fake(text: &str) -> String {
    let map: std::collections::HashMap<char, &str> = "a=а b=б c=ц d=д e=е f=ф g=г h=х i=и j=й k=к l=л m=м n=н o=о p=п q=к r=р s=с t=т u=у v=в w=в x=кс y=ы z=з"
        .split(' ')
        .filter_map(|p| p.split_once('=').map(|(a, b)| (a.chars().next().unwrap(), b)))
        .collect();
    let mut out = String::new();
    for c in text.chars() {
        match map.get(&c.to_ascii_lowercase()) {
            Some(r) if c.is_ascii_alphabetic() => {
                let r = if c.is_uppercase() {
                    r.to_uppercase()
                } else {
                    r.to_string()
                };
                out.push_str(&r);
                if "aeiou".contains(c.to_ascii_lowercase()) {
                    out.push('ь');
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).unwrap();
    let sel = args
        .get(4)
        .and_then(|spec| pdf::PageSelection::parse(spec).unwrap());
    let t = std::time::Instant::now();
    let analysis = pdf::analyze(&bytes, sel.as_ref()).unwrap();
    println!(
        "analyze {:?}: {} blocks",
        t.elapsed(),
        analysis.blocks.len()
    );
    for d in &analysis.diagnostics {
        println!("diag: {d}");
    }
    let filter: Option<String> = args.get(3).cloned();
    for b in analysis.blocks.iter().filter(|b| {
        filter
            .as_ref()
            .is_some_and(|f| b.id.starts_with(f.as_str()))
    }) {
        println!("{} {:?} {:?}", b.id, b.block_type, b.source_text);
    }
    let real: std::collections::HashMap<String, String> = std::env::var("PDF_TRANSLATIONS")
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    println!("real translations: {}", real.len());
    let mut blocks = analysis.blocks.clone();
    for b in &mut blocks {
        b.translated_text = Some(
            real.get(&b.id)
                .cloned()
                .unwrap_or_else(|| fake(&b.source_text)),
        );
    }
    let t = std::time::Instant::now();
    let out = pdf::export(&bytes, &blocks, sel.as_ref()).unwrap();
    println!("export {:?}: {} bytes", t.elapsed(), out.bytes.len());
    for d in &out.diagnostics {
        println!("diag: {d}");
    }
    std::fs::write(&args[2], out.bytes).unwrap();
}

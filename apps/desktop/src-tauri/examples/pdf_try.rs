//! Developer probe: analyze a PDF, fake-translate it (Cyrillic transliteration,
//! ~25% longer) and export, to inspect layout preservation without a model.
use lingvoloc_lib::documents::pdf;
use rusqlite::Connection;

fn load_job_translations(
    database: &std::path::Path,
    job_id: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let connection = Connection::open(database).map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT block_id, translated_text
             FROM document_blocks
             WHERE job_id = ?1
               AND translated_text IS NOT NULL
               AND trim(translated_text) <> ''
             ORDER BY ordinal",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([job_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

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
    let real = std::env::var("PDF_TRANSLATIONS")
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .or_else(|| {
            let database = std::env::var_os("PDF_JOB_DB")?;
            let job_id = std::env::var("PDF_JOB_ID").ok()?;
            match load_job_translations(std::path::Path::new(&database), &job_id) {
                Ok(translations) => Some(translations),
                Err(error) => {
                    eprintln!("could not load PDF job translations: {error}");
                    Some(std::collections::HashMap::new())
                }
            }
        })
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

#[cfg(test)]
mod tests {
    use super::load_job_translations;
    use rusqlite::Connection;

    #[test]
    fn loads_only_non_empty_translations_for_requested_job() {
        let path = std::env::temp_dir().join(format!(
            "lingvoloc-pdf-try-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let connection = Connection::open(&path).expect("open database");
        connection
            .execute_batch(
                "CREATE TABLE document_blocks (
                    job_id TEXT NOT NULL,
                    block_id TEXT NOT NULL,
                    ordinal INTEGER NOT NULL,
                    translated_text TEXT
                );
                INSERT INTO document_blocks VALUES
                    ('job-a', 'pdf#p0001-000', 1, 'second'),
                    ('job-a', 'pdf#p0001-001', 0, 'first'),
                    ('job-a', 'pdf#p0001-002', 2, '   '),
                    ('job-b', 'pdf#p0001-000', 0, 'other');",
            )
            .expect("seed database");
        drop(connection);

        let translations = load_job_translations(&path, "job-a").expect("load translations");
        assert_eq!(
            translations,
            std::collections::HashMap::from([
                ("pdf#p0001-000".into(), "second".into()),
                ("pdf#p0001-001".into(), "first".into()),
            ])
        );

        std::fs::remove_file(path).expect("remove temporary database");
    }
}

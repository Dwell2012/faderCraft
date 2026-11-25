use djcraft_library::Library;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let database = args.next().ok_or("database path needed")?;
    let source_id = args.next().ok_or("source id needed")?.parse()?;
    let mut library = Library::open(std::path::Path::new(&database))?;
    let mut last = 0;
    let summary = library.scan(source_id, |progress| {
        if progress.done >= last + 5_000 {
            eprintln!("Scanned {}/{}", progress.done, progress.total);
            last = progress.done;
        }
    })?;
    println!("{summary:?}");
    Ok(())
}

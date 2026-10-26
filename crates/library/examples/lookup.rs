//! Looks a track up with every provider that searches by name, and prints
//! the candidates. Discogs is included when DISCOGS_TOKEN is set.
//!
//!     cargo run -p djcraft-library --example lookup -- <artist> <title>
//! Add --details to inspect the first candidate's track and release details.

use djcraft_library::lookup;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().collect();
    let full = args.last().is_some_and(|arg| arg == "--details");
    if full {
        args.pop();
    }
    let [_, artist, title] = args.as_slice() else {
        return Err("usage: lookup <artist> <title> [--details]".into());
    };
    for provider in lookup::Provider::ALL {
        println!("{}:", provider.label());
        // Discogs needs a token: DISCOGS_TOKEN=... to include it.
        let keys = lookup::Keys {
            discogs_token: std::env::var("DISCOGS_TOKEN").unwrap_or_default(),
            ..Default::default()
        };
        if provider == lookup::Provider::Discogs && keys.discogs_token.is_empty() {
            continue;
        }
        for (index, found) in lookup::search(provider, artist, title, &keys)?
            .into_iter()
            .enumerate()
        {
            println!("  {found:?}");
            if full && index == 0 {
                let details = lookup::details(&found, &keys)?;
                for (field, value) in details.fields {
                    println!("    {field}: {value}");
                }
                println!("    Source: {}", details.source_url);
                for track in details.tracks {
                    println!("    {track}");
                }
            }
        }
    }
    Ok(())
}

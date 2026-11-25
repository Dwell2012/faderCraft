//! Published GitHub Releases are the update source of truth.

use serde::Deserialize;

const LATEST: &str = "https://api.github.com/repos/Dwell2012/faderCraft/releases/latest";

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

#[derive(Debug, PartialEq)]
pub enum Update {
    Current,
    Available { version: String, url: String },
}

fn version_parts(version: &str) -> Option<[u64; 3]> {
    let version = version.trim_start_matches('v');
    let version = version.split(['-', '+']).next()?;
    let mut numbers = version.split('.').map(str::parse::<u64>);
    let result = [
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
    ];
    numbers.next().is_none().then_some(result)
}

fn compare_release(current: &str, release: Release) -> Result<Update, String> {
    let current_version = version_parts(current).ok_or("Invalid installed version")?;
    let latest_version = version_parts(&release.tag_name).ok_or("Invalid release version")?;
    if latest_version <= current_version {
        return Ok(Update::Current);
    }
    if !release
        .html_url
        .starts_with("https://github.com/Dwell2012/faderCraft/releases/")
    {
        return Err("Unexpected release URL".into());
    }
    Ok(Update::Available {
        version: release.tag_name,
        url: release.html_url,
    })
}

pub fn check() -> Result<Update, String> {
    let response = ureq::get(LATEST)
        .header("User-Agent", "faderCraft-update-check")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("Could not check GitHub Releases: {error}"))?;
    let release: Release = response
        .into_body()
        .read_json()
        .map_err(|error| format!("Could not read release details: {error}"))?;
    compare_release(env!("CARGO_PKG_VERSION"), release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_version_components_and_rejects_foreign_links() {
        let release = |tag: &str, url: &str| Release {
            tag_name: tag.into(),
            html_url: url.into(),
        };
        let url = "https://github.com/Dwell2012/faderCraft/releases/tag/v1.10.0";
        assert_eq!(
            compare_release("1.9.9", release("v1.10.0", url)),
            Ok(Update::Available {
                version: "v1.10.0".into(),
                url: url.into()
            })
        );
        assert_eq!(
            compare_release("1.10.0", release("v1.9.9", url)),
            Ok(Update::Current)
        );
        assert!(compare_release("1.0.0", release("v2.0.0", "https://example.com")).is_err());
    }
}

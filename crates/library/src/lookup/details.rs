//! On-demand provider detail requests. These never touch music files or the library.
use super::*;
use serde_json::Value;

fn text(value: &Value, key: &str) -> String {
    match &value[key] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn list(value: &Value, key: &str) -> Vec<Value> {
    value[key].as_array().cloned().unwrap_or_default()
}

fn credit(value: &Value) -> String {
    list(value, "artist-credit")
        .iter()
        .map(|a| {
            let name = text(a, "name");
            format!(
                "{}{}",
                if name.is_empty() {
                    text(&a["artist"], "name")
                } else {
                    name
                },
                text(a, "joinphrase")
            )
        })
        .collect()
}

impl TrackDetails {
    fn add(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        if !value.trim().is_empty() {
            self.fields.push((key.into(), value));
        }
    }
}

fn json(
    provider: Provider,
    request: impl Fn() -> ureq::RequestBuilder<ureq::typestate::WithoutBody>,
) -> Result<Value> {
    serde_json::from_str(&get(provider, request)?).map_err(|error| {
        Error::Lookup(format!(
            "{} details were not understood: {error}",
            provider.label()
        ))
    })
}

/// Fetch details for the selected candidate. Missing credentials and failed detail
/// requests do not discard search results. IDs stay on fixed provider hosts.
pub fn details(found: &TrackMatch, keys: &Keys) -> Result<TrackDetails> {
    match &found.reference {
        Some(MatchReference::Recording(id, release_id)) => {
            if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                || !release_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err(Error::Lookup("Invalid MusicBrainz identifier".into()));
            }
            let recording = json(Provider::MusicBrainz, || {
                agent_request(&format!("https://musicbrainz.org/ws/2/recording/{id}"))
                    .query("fmt", "json")
                    .query(
                        "inc",
                        "artists+releases+release-groups+isrcs+tags+artist-rels",
                    )
            })?;
            let chosen = if release_id.is_empty() {
                list(&recording, "releases")
                    .into_iter()
                    .min_by_key(|r| {
                        let group = &r["release-group"];
                        (
                            !list(group, "secondary-types").is_empty(),
                            text(r, "date").is_empty(),
                            text(r, "date"),
                        )
                    })
                    .map(|r| text(&r, "id"))
                    .unwrap_or_default()
            } else {
                release_id.clone()
            };
            let release = if chosen.is_empty() {
                Value::Null
            } else {
                if !chosen
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
                {
                    return Err(Error::Lookup(
                        "Invalid MusicBrainz release identifier".into(),
                    ));
                }
                json(Provider::MusicBrainz, || {
                    agent_request(&format!("https://musicbrainz.org/ws/2/release/{chosen}"))
                        .query("fmt", "json")
                        .query("inc", "artists+recordings+labels+release-groups")
                })?
            };
            Ok(musicbrainz_details(&recording, &release))
        }
        Some(MatchReference::Discogs(id)) => {
            if keys.discogs_token.trim().is_empty() {
                return Err(Error::Lookup(
                    "Discogs needs a personal token in Settings, Library.".into(),
                ));
            }
            let release = json(Provider::Discogs, || {
                agent_request(&format!("https://api.discogs.com/releases/{id}")).header(
                    "Authorization",
                    format!("Discogs token={}", keys.discogs_token.trim()),
                )
            })?;
            Ok(discogs_details(&release, &found.title))
        }
        Some(MatchReference::Itunes(id)) => {
            let response = json(Provider::Itunes, || {
                agent_request("https://itunes.apple.com/lookup").query("id", id.to_string())
            })?;
            let track = list(&response, "results")
                .into_iter()
                .find(|r| r["trackId"].as_u64() == Some(*id))
                .ok_or_else(|| Error::Lookup("The iTunes track is no longer available.".into()))?;
            let album = if let Some(album_id) = track["collectionId"].as_u64() {
                json(Provider::Itunes, || {
                    agent_request("https://itunes.apple.com/lookup")
                        .query("id", album_id.to_string())
                        .query("entity", "song")
                })?
            } else {
                Value::Null
            };
            Ok(itunes_details(&track, &album))
        }
        None => Err(Error::Lookup(
            "This result has no provider identifier for additional details.".into(),
        )),
    }
}

fn duration(milliseconds: &Value) -> String {
    milliseconds
        .as_u64()
        .map(|ms| {
            let s = ms / 1000;
            format!("{}:{:02}", s / 60, s % 60)
        })
        .unwrap_or_default()
}

fn musicbrainz_details(recording: &Value, release: &Value) -> TrackDetails {
    let mut result = TrackDetails {
        artwork_url: crate::artwork::source_url(&text(release, "id")),
        source_url: format!(
            "https://musicbrainz.org/recording/{}",
            text(recording, "id")
        ),
        ..Default::default()
    };
    result.add("Title", text(recording, "title"));
    result.add("Artist", credit(recording));
    for relation in list(recording, "relations") {
        let artist = text(&relation["artist"], "name");
        if !artist.is_empty() {
            result.add("Credit", format!("{}: {}", text(&relation, "type"), artist));
        }
    }
    result.add(
        "Version / disambiguation",
        text(recording, "disambiguation"),
    );
    result.add("Duration", duration(&recording["length"]));
    result.add("First release date", text(recording, "first-release-date"));
    result.add("Album / release", text(release, "title"));
    result.add("Album artist", credit(release));
    result.add("Release date", text(release, "date"));
    result.add("Country", text(release, "country"));
    result.add("Status", text(release, "status"));
    result.add("Barcode", text(release, "barcode"));
    result.add(
        "Release type",
        text(&release["release-group"], "primary-type"),
    );
    result.add(
        "Genres / tags",
        list(recording, "tags")
            .iter()
            .map(|t| text(t, "name"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    result.add(
        "ISRC",
        list(recording, "isrcs")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
    );
    for info in list(release, "label-info") {
        result.add(
            "Label / catalogue",
            [text(&info["label"], "name"), text(&info, "catalog-number")]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
        );
    }
    let recording_id = text(recording, "id");
    for medium in list(release, "media") {
        result.add("Format", text(&medium, "format"));
        for track in list(&medium, "tracks") {
            let position = format!("{}.{}", text(&medium, "position"), text(&track, "number"));
            let title = text(&track, "title");
            let artist = credit(&track);
            result.tracks.push(format!(
                "{position}  {title}  {artist}  {}",
                duration(&track["length"])
            ));
            if !recording_id.is_empty() && text(&track["recording"], "id") == recording_id {
                result.add("Track / disc", position);
                result.matched_title = Some(title);
                result.matched_artist = Some(artist).filter(|s| !s.is_empty());
            }
        }
    }
    result.add("MusicBrainz recording ID", recording_id);
    result.add("MusicBrainz release ID", text(release, "id"));
    result
}

fn discogs_credit(value: &Value, key: &str) -> String {
    list(value, key)
        .iter()
        .map(|a| {
            let name = text(a, "anv");
            let name = if name.is_empty() {
                text(a, "name")
            } else {
                name
            };
            let role = text(a, "role");
            if role.is_empty() {
                name
            } else {
                format!("{name} ({role})")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn discogs_details(release: &Value, wanted: &str) -> TrackDetails {
    let mut result = TrackDetails {
        source_url: format!("https://www.discogs.com/release/{}", text(release, "id")),
        ..Default::default()
    };
    result.add("Album / release", text(release, "title"));
    result.add("Release artist", discogs_credit(release, "artists"));
    result.add("Release date", text(release, "released"));
    result.add("Country", text(release, "country"));
    for key in ["genres", "styles"] {
        result.add(
            key,
            list(release, key)
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    for label in list(release, "labels") {
        result.add(
            "Label / catalogue",
            format!("{} · {}", text(&label, "name"), text(&label, "catno")),
        );
    }
    for format in list(release, "formats") {
        result.add(
            "Format",
            [
                text(&format, "name"),
                list(&format, "descriptions")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
            ]
            .join(" · "),
        );
    }
    for identifier in list(release, "identifiers") {
        result.add(&text(&identifier, "type"), text(&identifier, "value"));
    }
    result.add("Credits", discogs_credit(release, "extraartists"));
    let mut tracks = Vec::new();
    for track in list(release, "tracklist") {
        if track["type_"] == "heading" {
            continue;
        }
        let children = list(&track, "sub_tracks");
        if children.is_empty() {
            tracks.push(track);
        } else {
            tracks.extend(children);
        }
    }
    for track in &tracks {
        result.tracks.push(format!(
            "{}  {}  {}  {}",
            text(track, "position"),
            text(track, "title"),
            discogs_credit(track, "artists"),
            text(track, "duration")
        ));
    }
    let matched = tracks.iter().max_by(|a, b| {
        similarity(wanted, &text(a, "title")).total_cmp(&similarity(wanted, &text(b, "title")))
    });
    if let Some(track) = matched.filter(|t| similarity(wanted, &text(t, "title")) >= 0.6) {
        let title = text(track, "title");
        result.add("Matched track", title.clone());
        result.add("Track position", text(track, "position"));
        result.add("Duration", text(track, "duration"));
        result.add("Track credits", discogs_credit(track, "extraartists"));
        result.matched_title = Some(title);
        result.matched_artist = Some(discogs_credit(track, "artists")).filter(|s| !s.is_empty());
    } else {
        result.add(
            "Track match",
            "No confident track match in this release; inspect the track list.",
        );
    }
    result.add("Notes", text(release, "notes"));
    result
}

fn itunes_details(track: &Value, album: &Value) -> TrackDetails {
    let url = text(track, "trackViewUrl");
    let mut result = TrackDetails {
        source_url: if url.starts_with("https://music.apple.com/")
            || url.starts_with("https://itunes.apple.com/")
        {
            url
        } else {
            String::new()
        },
        ..Default::default()
    };
    for (label, key) in [
        ("Title", "trackName"),
        ("Artist", "artistName"),
        ("Album / release", "collectionName"),
        ("Release date", "releaseDate"),
        ("Genre", "primaryGenreName"),
        ("Track number", "trackNumber"),
        ("Track count", "trackCount"),
        ("Disc number", "discNumber"),
        ("Disc count", "discCount"),
        ("Advisory", "trackExplicitness"),
        ("Store country", "country"),
        ("iTunes track ID", "trackId"),
        ("iTunes album ID", "collectionId"),
    ] {
        result.add(label, text(track, key));
    }
    result.add("Duration", duration(&track["trackTimeMillis"]));
    for entry in list(album, "results") {
        if entry["wrapperType"] == "collection" {
            result.add("Copyright", text(&entry, "copyright"));
            result.add("Album artist", text(&entry, "artistName"));
        } else if entry["kind"] == "song" {
            result.tracks.push(format!(
                "{}.{}  {}  {}  {}",
                text(&entry, "discNumber"),
                text(&entry, "trackNumber"),
                text(&entry, "trackName"),
                text(&entry, "artistName"),
                duration(&entry["trackTimeMillis"])
            ));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn musicbrainz_keeps_recording_and_release_dates_distinct_and_locates_track() {
        let recording = json!({"id":"recording", "title":"Song (Extended Mix)", "length":420000,
            "first-release-date":"1999-04", "artist-credit":[{"name":"Artist"}],
            "isrcs":["GB1239900001"],"tags":[{"name":"trance"}]});
        let release = json!({"id":"release", "title":"Reissue", "date":"2020-05-12",
            "country":"GB", "barcode":"123456", "artist-credit":[{"artist":{"name":"Artist"}}],
            "label-info":[{"label":{"name":"Label"},"catalog-number":"CAT001"}],
            "media":[{"position":2,"format":"CD","tracks":[
                {"number":"3","title":"Song (Extended Mix)","length":420000,
                 "recording":{"id":"recording"},"artist-credit":[{"name":"Artist"}]}]}]});
        let details = musicbrainz_details(&recording, &release);
        for pair in [
            ("First release date", "1999-04"),
            ("Release date", "2020-05-12"),
            ("Track / disc", "2.3"),
            ("Duration", "7:00"),
            ("Label / catalogue", "Label · CAT001"),
            ("ISRC", "GB1239900001"),
        ] {
            assert!(
                details.fields.contains(&(pair.0.into(), pair.1.into())),
                "{pair:?}"
            );
        }
        assert_eq!(
            details.matched_title.as_deref(),
            Some("Song (Extended Mix)")
        );
        assert_eq!(details.tracks.len(), 1);
    }

    #[test]
    fn discogs_reads_actual_track_credit_and_version_from_compilation() {
        let release = json!({"id":123,"title":"Compilation", "released":"2001-04-01",
            "artists":[{"name":"Various"}], "styles":["Trance"],
            "tracklist":[{"type_":"heading","title":"Side A"},
                {"position":"A1","title":"Song (Radio Edit)","duration":"3:15","artists":[{"name":"Actual Artist"}]},
                {"position":"A2","title":"Song (Extended Mix)","duration":"7:00","artists":[{"name":"Actual Artist"}],
                 "extraartists":[{"name":"Remixer","role":"Remix"}]}]});
        let details = discogs_details(&release, "Song (Extended Mix)");
        assert_eq!(
            details.matched_title.as_deref(),
            Some("Song (Extended Mix)")
        );
        assert_eq!(details.matched_artist.as_deref(), Some("Actual Artist"));
        assert_eq!(details.tracks.len(), 2);
        assert!(
            details
                .fields
                .contains(&("Track credits".into(), "Remixer (Remix)".into()))
        );
        assert_eq!(details.source_url, "https://www.discogs.com/release/123");
        let unmatched = discogs_details(&release, "Completely unrelated title");
        assert_eq!(unmatched.matched_title, None);
        assert_eq!(unmatched.matched_artist, None);
        assert!(
            unmatched
                .fields
                .iter()
                .any(|(label, _)| label == "Track match")
        );
    }

    #[test]
    fn itunes_shows_track_numbers_duration_and_album_tracklist() {
        let track = json!({"trackName":"Song","artistName":"Artist","collectionName":"Album",
            "trackId":12,"collectionId":34,"trackNumber":2,"discNumber":1,"trackTimeMillis":215000,
            "releaseDate":"2000-01-02T00:00:00Z","trackViewUrl":"https://music.apple.com/gb/album/song/34?i=12"});
        let album = json!({"results":[{"wrapperType":"collection","copyright":"2000 Label","artistName":"Album Artist"},
            {"kind":"song","discNumber":1,"trackNumber":2,"trackName":"Song","artistName":"Artist","trackTimeMillis":215000}]});
        let details = itunes_details(&track, &album);
        assert!(details.fields.contains(&("Duration".into(), "3:35".into())));
        assert!(
            details
                .fields
                .contains(&("Copyright".into(), "2000 Label".into()))
        );
        assert!(
            details
                .fields
                .contains(&("Album artist".into(), "Album Artist".into()))
        );
        assert_eq!(details.tracks.len(), 1);
        assert!(itunes_details(&Value::Null, &Value::Null).fields.is_empty());
    }

    #[test]
    fn invalid_identifiers_are_rejected_before_network_access() {
        let found = TrackMatch {
            reference: Some(MatchReference::Recording("../bad".into(), String::new())),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            year: None,
            genre: None,
            score: 0,
            compilation: false,
            provider: Provider::MusicBrainz,
        };
        assert!(
            details(&found, &Keys::default())
                .unwrap_err()
                .to_string()
                .contains("identifier")
        );
    }
}

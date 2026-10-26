use djcraft_protocol::{HOT_CUES, HotCue};
use std::collections::BTreeMap;
use std::path::Path;
pub type CueStore = BTreeMap<String, [Option<HotCue>; HOT_CUES]>;
pub fn load(root: &Path) -> CueStore {
    crate::track_identity::normalize(
        root,
        "hot-cues.json",
        std::fs::read(root.join("hot-cues.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default(),
    )
}
pub fn save(root: &Path, cues: &CueStore) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = root.join("hot-cues.json.tmp");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(cues).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(path, root.join("hot-cues.json")).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cues_and_loop_cues_survive_save_replace_and_reload() {
        let root = std::env::temp_dir().join(format!("djcraft-pad-test-{}", std::process::id()));
        let mut cues = CueStore::new();
        let mut slots = [None; HOT_CUES];
        slots[0] = Some(HotCue {
            seconds: 1.25,
            loop_end: None,
        });
        slots[15] = Some(HotCue {
            seconds: 4.,
            loop_end: Some(8.),
        });
        cues.insert("test.mp3".into(), slots);
        save(&root, &cues).unwrap();
        assert_eq!(load(&root), cues);
        cues.get_mut("test.mp3").unwrap()[0] = None;
        save(&root, &cues).unwrap();
        assert_eq!(load(&root), cues);
        std::fs::remove_file(root.join("hot-cues.json")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

pub type AppearanceStore = BTreeMap<String, [djcraft_protocol::CueAppearance; HOT_CUES]>;
pub fn load_appearances(root: &Path) -> AppearanceStore {
    crate::track_identity::normalize(
        root,
        "cue-appearance.json",
        std::fs::read(root.join("cue-appearance.json"))
            .ok()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default(),
    )
}
pub fn save_appearances(root: &Path, styles: &AppearanceStore) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let tmp = root.join("cue-appearance.json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(styles).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, root.join("cue-appearance.json")).map_err(|e| e.to_string())
}
/// Distinct shades of lime/green for A and sapphire/cyan for B, across 16 slots.
pub fn cue_colour(deck: usize, slot: usize, style: &djcraft_protocol::CueAppearance) -> u32 {
    const A: [u32; 16] = [
        0xc8ff38, 0x67e85b, 0xffdf45, 0x35d89c, 0xa5dc28, 0x99f578, 0xe9ef65, 0x54e7c0, 0x7fbf30,
        0x39b878, 0xd2b936, 0x38bca1, 0xb7e76c, 0x71d895, 0xe1cf85, 0x92f0cc,
    ];
    const B: [u32; 16] = [
        0x4777ff, 0x35c6ed, 0x8673f5, 0x459be8, 0x5261d8, 0x64dce4, 0xb09af5, 0x7abaff, 0x627ce0,
        0x399fba, 0x8e64d9, 0x3686ca, 0x91a6ff, 0x8ad9ee, 0xc2b2ff, 0x74a1cc,
    ];
    style.colour.unwrap_or(if deck == 0 {
        A[slot % 16]
    } else {
        B[slot % 16]
    })
}

#[cfg(test)]
mod appearance_tests {
    use super::*;
    #[test]
    fn cue_appearance_round_trips_and_palettes_are_distinct() {
        let root =
            std::env::temp_dir().join(format!("djcraft-appearance-test-{}", std::process::id()));
        let mut styles = AppearanceStore::new();
        let mut slots: [djcraft_protocol::CueAppearance; HOT_CUES] = Default::default();
        slots[3].label = "Drop".into();
        slots[3].colour = Some(0x123456);
        styles.insert("track.mp3".into(), slots);
        save_appearances(&root, &styles).unwrap();
        assert_eq!(load_appearances(&root), styles);
        for deck in 0..2 {
            let colours: std::collections::BTreeSet<_> = (0..16)
                .map(|i| cue_colour(deck, i, &Default::default()))
                .collect();
            assert_eq!(colours.len(), 16);
        }
        std::fs::remove_file(root.join("cue-appearance.json")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

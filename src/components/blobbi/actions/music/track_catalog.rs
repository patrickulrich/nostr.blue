#[derive(Clone, Debug, PartialEq)]
pub struct BlobbiTrack {
    pub id: &'static str,
    pub title: &'static str,
    pub artist: &'static str,
    pub url: &'static str,
    pub duration_secs: u32,
    pub tags: &'static [&'static str],
}

pub static TRACKS: &[BlobbiTrack] = &[
    BlobbiTrack {
        id: "lullaby-dreams",
        title: "Lullaby Dreams",
        artist: "BlobbiFM",
        url: "https://blossom.primal.net/lullaby-dreams.m4a",
        duration_secs: 180,
        tags: &["sleep", "calm"],
    },
    BlobbiTrack {
        id: "happy-bounce",
        title: "Happy Bounce",
        artist: "BlobbiFM",
        url: "https://blossom.primal.net/happy-bounce.m4a",
        duration_secs: 210,
        tags: &["happy", "play"],
    },
    BlobbiTrack {
        id: "adventure-time",
        title: "Adventure Time",
        artist: "BlobbiFM",
        url: "https://blossom.primal.net/adventure-time.m4a",
        duration_secs: 195,
        tags: &["adventure", "energy"],
    },
    BlobbiTrack {
        id: "rainy-day",
        title: "Rainy Day",
        artist: "BlobbiFM",
        url: "https://blossom.primal.net/rainy-day.m4a",
        duration_secs: 240,
        tags: &["calm", "sleep"],
    },
    BlobbiTrack {
        id: "sunshine-walk",
        title: "Sunshine Walk",
        artist: "BlobbiFM",
        url: "https://blossom.primal.net/sunshine-walk.m4a",
        duration_secs: 200,
        tags: &["happy", "outdoor"],
    },
    BlobbiTrack {
        id: "playtime-fun",
        title: "Playtime Fun",
        artist: "BlobbiFM",
        url: "https://blossom.primal.net/playtime-fun.m4a",
        duration_secs: 165,
        tags: &["play", "energy"],
    },
];

pub fn get_track_by_id(id: &str) -> Option<&'static BlobbiTrack> {
    TRACKS.iter().find(|t| t.id == id)
}

#[allow(dead_code)]
pub fn all_tracks() -> &'static [BlobbiTrack] {
    TRACKS
}

pub fn format_duration(secs: u32) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02}")
}

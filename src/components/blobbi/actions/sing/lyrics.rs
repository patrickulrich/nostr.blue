pub struct LyricSet {
    pub title: &'static str,
    pub lines: &'static [&'static str],
}

pub static LYRICS: &[LyricSet] = &[
    LyricSet {
        title: "Blobbi Lullaby",
        lines: &[
            "Close your eyes, little one",
            "The stars are shining bright",
            "Rest your head, the night has come",
            "Everything will be alright",
        ],
    },
    LyricSet {
        title: "Happy Blobbi Song",
        lines: &[
            "Bounce and play all day!",
            "Jump around, it's time to play",
            "Happy Blobbi, hip hooray!",
            "Every day's a brand new day",
        ],
    },
    LyricSet {
        title: "Adventure Time",
        lines: &[
            "Beyond the garden we will go",
            "Through the flowers, row by row",
            "Adventure calls, we can't say no",
            "The world is ours to get to know",
        ],
    },
    LyricSet {
        title: "Breakfast Song",
        lines: &[
            "Yummy yummy in my tummy",
            "Apples, burgers, cakes, oh my!",
            "Feed me well and watch me grow",
            "A happy tummy's all I know",
        ],
    },
    LyricSet {
        title: "Rainy Day",
        lines: &[
            "Pitter patter on the roof",
            "Cozy inside, safe and warm",
            "Raindrops make a gentle tune",
            "We'll be dry again real soon",
        ],
    },
    LyricSet {
        title: "Sunshine Song",
        lines: &[
            "Golden rays upon my face",
            "Warmth and love fill every space",
            "Sunshine makes the world so bright",
            "Everything feels just right",
        ],
    },
    LyricSet {
        title: "Bedtime Blues",
        lines: &[
            "One more story, then I'll sleep",
            "Counting sheep that leap and leap",
            "Dreams of places far away",
            "I'll be ready for a brand new day",
        ],
    },
    LyricSet {
        title: "Play Time",
        lines: &[
            "Throw the ball and watch it bounce",
            "Toys and games make me announce",
            "Laughter fills the air so free",
            "Play time is the best for me!",
        ],
    },
];

pub fn random_lyrics() -> &'static LyricSet {
    let seed = crate::platform::timestamp::now_millis() as usize;
    &LYRICS[seed % LYRICS.len()]
}

#[allow(dead_code)]
pub fn all_lyrics() -> &'static [LyricSet] {
    LYRICS
}

pub fn format_lyrics(set: &LyricSet) -> String {
    set.lines.join("\n")
}

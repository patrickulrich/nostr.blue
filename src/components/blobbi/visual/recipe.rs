use crate::utils::nip_bb::BlobbiStats;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub enum EyeType {
    #[default]
    Happy,
    Sad,
    Tired,
    Hungry,
    Bored,
    Excited,
    Sleeping,
    Angry,
    Love,
    Content,
    Watery,
    Star,
    Dizzy,
    SleepyBlink,
    Surprised,
    Curious,
    Mischievous,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub enum MouthType {
    #[default]
    Smile,
    Frown,
    Open,
    Sleeping,
    Neutral,
    Grin,
    Sad,
    Droopy,
    Sleepy,
    Round,
    Small,
    Smirk,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub enum BodyEffect {
    #[default]
    None,
    Dirt,
    Stink,
    Food,
    Sparkle,
    Sleeping,
    Excited,
    Music,
    Singing,
    Love,
    AngerRise,
    StinkFlies,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub enum AnimationType {
    #[default]
    Idle,
    Bounce,
    Sleep,
    Eating,
    Playing,
    Sad,
    Excited,
    Breathe,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extras {
    pub tears: Option<TearConfig>,
    pub drool: bool,
    pub food_icon: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TearConfig {
    pub eye: TearEye,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub enum TearEye {
    #[default]
    Both,
    Left,
    Right,
    Alternating,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EyebrowConfig {
    pub angle: f64,
    pub offset_y: f64,
    pub worried: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StatSeverity {
    Normal,
    Warning,
    High,
    Critical,
}

impl StatSeverity {
    pub fn from_value(value: f64) -> Self {
        if value < 30.0 {
            StatSeverity::Critical
        } else if value < 50.0 {
            StatSeverity::High
        } else if value < 70.0 {
            StatSeverity::Warning
        } else {
            StatSeverity::Normal
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub enum EmotionPreset {
    #[default]
    Neutral,
    Sad,
    Boring,
    Dirty,
    Happy,
    Angry,
    Surprised,
    Sleepy,
    Curious,
    Dizzy,
    Excited,
    ExcitedB,
    Mischievous,
    Adoring,
    Hungry,
}

impl EmotionPreset {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmotionPreset::Neutral => "neutral",
            EmotionPreset::Sad => "sad",
            EmotionPreset::Boring => "boring",
            EmotionPreset::Dirty => "dirty",
            EmotionPreset::Happy => "happy",
            EmotionPreset::Angry => "angry",
            EmotionPreset::Surprised => "surprised",
            EmotionPreset::Sleepy => "sleepy",
            EmotionPreset::Curious => "curious",
            EmotionPreset::Dizzy => "dizzy",
            EmotionPreset::Excited => "excited",
            EmotionPreset::ExcitedB => "excitedB",
            EmotionPreset::Mischievous => "mischievous",
            EmotionPreset::Adoring => "adoring",
            EmotionPreset::Hungry => "hungry",
        }
    }

    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "neutral" => Some(EmotionPreset::Neutral),
            "sad" => Some(EmotionPreset::Sad),
            "boring" => Some(EmotionPreset::Boring),
            "dirty" => Some(EmotionPreset::Dirty),
            "happy" => Some(EmotionPreset::Happy),
            "angry" => Some(EmotionPreset::Angry),
            "surprised" => Some(EmotionPreset::Surprised),
            "sleepy" => Some(EmotionPreset::Sleepy),
            "curious" => Some(EmotionPreset::Curious),
            "dizzy" => Some(EmotionPreset::Dizzy),
            "excited" => Some(EmotionPreset::Excited),
            "excitedB" => Some(EmotionPreset::ExcitedB),
            "mischievous" => Some(EmotionPreset::Mischievous),
            "adoring" => Some(EmotionPreset::Adoring),
            "hungry" => Some(EmotionPreset::Hungry),
            _ => None,
        }
    }

    pub fn all() -> &'static [EmotionPreset] {
        &[
            EmotionPreset::Neutral,
            EmotionPreset::Sad,
            EmotionPreset::Boring,
            EmotionPreset::Dirty,
            EmotionPreset::Happy,
            EmotionPreset::Angry,
            EmotionPreset::Surprised,
            EmotionPreset::Sleepy,
            EmotionPreset::Curious,
            EmotionPreset::Dizzy,
            EmotionPreset::Excited,
            EmotionPreset::ExcitedB,
            EmotionPreset::Mischievous,
            EmotionPreset::Adoring,
            EmotionPreset::Hungry,
        ]
    }
}

pub fn emotion_recipe(emotion: EmotionPreset) -> ComposableRecipe {
    match emotion {
        EmotionPreset::Neutral => ComposableRecipe {
            eye_type: EyeType::Content,
            mouth_type: MouthType::Smile,
            eyebrow: None,
            body_effects: vec![],
            extras: Extras::default(),
            animation: AnimationType::Idle,
        },
        EmotionPreset::Sad => ComposableRecipe {
            eye_type: EyeType::Watery,
            mouth_type: MouthType::Sad,
            eyebrow: Some(EyebrowConfig { angle: -18.0, offset_y: -2.0, worried: true }),
            body_effects: vec![],
            extras: Extras {
                tears: Some(TearConfig { eye: TearEye::Alternating }),
                drool: false,
                food_icon: false,
            },
            animation: AnimationType::Sad,
        },
        EmotionPreset::Boring => ComposableRecipe {
            eye_type: EyeType::Bored,
            mouth_type: MouthType::Droopy,
            eyebrow: Some(EyebrowConfig { angle: 0.0, offset_y: 3.0, worried: false }),
            body_effects: vec![],
            extras: Extras::default(),
            animation: AnimationType::Idle,
        },
        EmotionPreset::Dirty => ComposableRecipe {
            eye_type: EyeType::Content,
            mouth_type: MouthType::Droopy,
            eyebrow: Some(EyebrowConfig { angle: 10.0, offset_y: -1.0, worried: false }),
            body_effects: vec![BodyEffect::Dirt, BodyEffect::Stink],
            extras: Extras::default(),
            animation: AnimationType::Idle,
        },
        EmotionPreset::Happy => ComposableRecipe {
            eye_type: EyeType::Happy,
            mouth_type: MouthType::Smile,
            eyebrow: None,
            body_effects: vec![],
            extras: Extras::default(),
            animation: AnimationType::Bounce,
        },
        EmotionPreset::Angry => ComposableRecipe {
            eye_type: EyeType::Content,
            mouth_type: MouthType::Sad,
            eyebrow: Some(EyebrowConfig { angle: 22.0, offset_y: -2.0, worried: false }),
            body_effects: vec![BodyEffect::AngerRise],
            extras: Extras::default(),
            animation: AnimationType::Idle,
        },
        EmotionPreset::Surprised => ComposableRecipe {
            eye_type: EyeType::Surprised,
            mouth_type: MouthType::Round,
            eyebrow: Some(EyebrowConfig { angle: -15.0, offset_y: -5.0, worried: false }),
            body_effects: vec![],
            extras: Extras::default(),
            animation: AnimationType::Bounce,
        },
        EmotionPreset::Sleepy => ComposableRecipe {
            eye_type: EyeType::SleepyBlink,
            mouth_type: MouthType::Sleepy,
            eyebrow: None,
            body_effects: vec![],
            extras: Extras::default(),
            animation: AnimationType::Breathe,
        },
        EmotionPreset::Curious => ComposableRecipe {
            eye_type: EyeType::Curious,
            mouth_type: MouthType::Small,
            eyebrow: Some(EyebrowConfig { angle: -8.0, offset_y: -2.0, worried: false }),
            body_effects: vec![],
            extras: Extras::default(),
            animation: AnimationType::Idle,
        },
        EmotionPreset::Dizzy => ComposableRecipe {
            eye_type: EyeType::Dizzy,
            mouth_type: MouthType::Round,
            eyebrow: Some(EyebrowConfig { angle: -10.0, offset_y: -3.0, worried: true }),
            body_effects: vec![],
            extras: Extras::default(),
            animation: AnimationType::Idle,
        },
        EmotionPreset::Excited => ComposableRecipe {
            eye_type: EyeType::Star,
            mouth_type: MouthType::Grin,
            eyebrow: None,
            body_effects: vec![BodyEffect::Excited],
            extras: Extras::default(),
            animation: AnimationType::Excited,
        },
        EmotionPreset::ExcitedB => ComposableRecipe {
            eye_type: EyeType::Star,
            mouth_type: MouthType::Round,
            eyebrow: None,
            body_effects: vec![BodyEffect::Excited],
            extras: Extras::default(),
            animation: AnimationType::Excited,
        },
        EmotionPreset::Mischievous => ComposableRecipe {
            eye_type: EyeType::Content,
            mouth_type: MouthType::Smirk,
            eyebrow: Some(EyebrowConfig { angle: 12.0, offset_y: -1.0, worried: false }),
            body_effects: vec![],
            extras: Extras::default(),
            animation: AnimationType::Bounce,
        },
        EmotionPreset::Adoring => ComposableRecipe {
            eye_type: EyeType::Watery,
            mouth_type: MouthType::Small,
            eyebrow: None,
            body_effects: vec![BodyEffect::Love],
            extras: Extras::default(),
            animation: AnimationType::Breathe,
        },
        EmotionPreset::Hungry => ComposableRecipe {
            eye_type: EyeType::Watery,
            mouth_type: MouthType::Small,
            eyebrow: Some(EyebrowConfig { angle: -14.0, offset_y: -1.0, worried: true }),
            body_effects: vec![],
            extras: Extras {
                tears: None,
                drool: true,
                food_icon: true,
            },
            animation: AnimationType::Idle,
        },
    }
}

pub fn merge_recipes(base: &ComposableRecipe, overlay: &ComposableRecipe) -> ComposableRecipe {
    ComposableRecipe {
        eye_type: overlay.eye_type,
        mouth_type: overlay.mouth_type,
        eyebrow: overlay.eyebrow.clone().or_else(|| base.eyebrow.clone()),
        body_effects: if overlay.body_effects.is_empty() {
            base.body_effects.clone()
        } else {
            overlay.body_effects.clone()
        },
        extras: Extras {
            tears: overlay.extras.tears.clone().or_else(|| base.extras.tears.clone()),
            drool: overlay.extras.drool || base.extras.drool,
            food_icon: overlay.extras.food_icon || base.extras.food_icon,
        },
        animation: overlay.animation,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComposableRecipe {
    pub eye_type: EyeType,
    pub mouth_type: MouthType,
    pub eyebrow: Option<EyebrowConfig>,
    pub body_effects: Vec<BodyEffect>,
    pub extras: Extras,
    pub animation: AnimationType,
}

pub fn resolve_composable(stats: &BlobbiStats, is_sleeping: bool, _mood: &str) -> ComposableRecipe {
    if is_sleeping {
        let awake_recipe = resolve_awake(stats);
        return ComposableRecipe {
            eye_type: EyeType::Sleeping,
            mouth_type: MouthType::Sleeping,
            eyebrow: None,
            body_effects: {
                let mut effects: Vec<BodyEffect> = awake_recipe
                    .body_effects
                    .iter()
                    .filter(|e| matches!(e, BodyEffect::Dirt | BodyEffect::Stink | BodyEffect::StinkFlies))
                    .cloned()
                    .collect();
                effects.push(BodyEffect::Sleeping);
                effects
            },
            extras: Extras::default(),
            animation: AnimationType::Sleep,
        };
    }

    resolve_awake(stats)
}

fn resolve_awake(stats: &BlobbiStats) -> ComposableRecipe {

    let severities = [
        ("energy", stats.energy, StatSeverity::from_value(stats.energy)),
        ("health", stats.health, StatSeverity::from_value(stats.health)),
        ("hunger", stats.hunger, StatSeverity::from_value(stats.hunger)),
        ("hygiene", stats.hygiene, StatSeverity::from_value(stats.hygiene)),
        ("happiness", stats.happiness, StatSeverity::from_value(stats.happiness)),
    ];

    let avg = stats.average();

    let mut eye_type = EyeType::Happy;
    let mut eye_priority: u32 = 0;
    let mut mouth_type = MouthType::Smile;
    let mut mouth_priority: u32 = 0;
    let mut eyebrow: Option<EyebrowConfig> = None;
    let mut brow_priority: u32 = 0;
    let mut body_effects: Vec<BodyEffect> = Vec::new();
    let mut extras = Extras::default();

    for (stat, _value, severity) in &severities {
        if matches!(severity, StatSeverity::Normal) {
            continue;
        }

        let priority = match severity {
            StatSeverity::Critical => 4,
            StatSeverity::High => 3,
            StatSeverity::Warning => 2,
            StatSeverity::Normal => 0,
        };

        match *stat {
            "energy" => {
                let (eye, mouth) = match severity {
                    StatSeverity::Warning => (EyeType::SleepyBlink, MouthType::Sleepy),
                    StatSeverity::High => (EyeType::SleepyBlink, MouthType::Sleepy),
                    StatSeverity::Critical => (EyeType::SleepyBlink, MouthType::Sleepy),
                    _ => (EyeType::Happy, MouthType::Smile),
                };
                if priority > eye_priority {
                    eye_type = eye;
                    eye_priority = priority;
                }
                if priority > mouth_priority {
                    mouth_type = mouth;
                    mouth_priority = priority;
                }
            }
            "health" => {
                let (eye, mouth, brow) = match severity {
                    StatSeverity::Warning => (eye_type, MouthType::Sad, EyebrowConfig { angle: -5.0, offset_y: -2.0, worried: true }),
                    StatSeverity::High => (eye_type, MouthType::Sad, EyebrowConfig { angle: -10.0, offset_y: -3.0, worried: true }),
                    StatSeverity::Critical => (EyeType::Dizzy, MouthType::Round, EyebrowConfig { angle: -15.0, offset_y: -4.0, worried: true }),
                    _ => (eye_type, mouth_type, EyebrowConfig::default()),
                };
                let health_eye_priority = match severity {
                    StatSeverity::Critical => 5,
                    _ => priority,
                };
                if health_eye_priority > eye_priority {
                    eye_type = eye;
                    eye_priority = health_eye_priority;
                }
                if priority > mouth_priority {
                    mouth_type = mouth;
                    mouth_priority = priority;
                }
                if priority > brow_priority {
                    eyebrow = Some(brow);
                    brow_priority = priority;
                }
            }
            "hunger" => {
                let (eye, mouth, brow) = match severity {
                    StatSeverity::Warning => (EyeType::Watery, MouthType::Small, EyebrowConfig { angle: 8.0, offset_y: -1.0, worried: false }),
                    StatSeverity::High => (EyeType::Watery, MouthType::Small, EyebrowConfig { angle: 12.0, offset_y: -2.0, worried: true }),
                    StatSeverity::Critical => (EyeType::Watery, MouthType::Droopy, EyebrowConfig { angle: 15.0, offset_y: -3.0, worried: true }),
                    _ => (EyeType::Happy, MouthType::Smile, EyebrowConfig::default()),
                };
                if priority > eye_priority {
                    eye_type = eye;
                    eye_priority = priority;
                }
                if priority > mouth_priority {
                    mouth_type = mouth;
                    mouth_priority = priority;
                }
                if priority > brow_priority {
                    eyebrow = Some(brow);
                    brow_priority = priority;
                }
                extras.drool = true;
                extras.food_icon = true;
            }
            "hygiene" => {
                let (mouth, brow) = match severity {
                    StatSeverity::Warning => (MouthType::Droopy, EyebrowConfig { angle: -3.0, offset_y: -1.0, worried: false }),
                    StatSeverity::High => (MouthType::Droopy, EyebrowConfig { angle: -8.0, offset_y: -2.0, worried: true }),
                    StatSeverity::Critical => (MouthType::Droopy, EyebrowConfig { angle: -12.0, offset_y: -3.0, worried: true }),
                    _ => (MouthType::Smile, EyebrowConfig::default()),
                };
                if priority > mouth_priority {
                    mouth_type = mouth;
                    mouth_priority = priority;
                }
                if priority > brow_priority {
                    eyebrow = Some(brow);
                    brow_priority = priority;
                }
                let hygiene_effect = match severity {
                    StatSeverity::Warning => Some(BodyEffect::Dirt),
                    StatSeverity::High => Some(BodyEffect::Dirt),
                    StatSeverity::Critical => Some(BodyEffect::Stink),
                    _ => None,
                };
                if let Some(eff) = hygiene_effect {
                    body_effects.push(eff);
                }
                if matches!(severity, StatSeverity::Critical) {
                    body_effects.push(BodyEffect::StinkFlies);
                }
            }
            "happiness" => {
                let (eye, mouth, brow) = match severity {
                    StatSeverity::Warning => (EyeType::Watery, MouthType::Sad, EyebrowConfig { angle: -5.0, offset_y: -2.0, worried: true }),
                    StatSeverity::High => (EyeType::Watery, MouthType::Sad, EyebrowConfig { angle: -10.0, offset_y: -3.0, worried: true }),
                    StatSeverity::Critical => (EyeType::Watery, MouthType::Sad, EyebrowConfig { angle: -15.0, offset_y: -4.0, worried: true }),
                    _ => (EyeType::Happy, MouthType::Smile, EyebrowConfig::default()),
                };
                if priority > eye_priority {
                    eye_type = eye;
                    eye_priority = priority;
                }
                if priority > mouth_priority {
                    mouth_type = mouth;
                    mouth_priority = priority;
                }
                if priority > brow_priority {
                    eyebrow = Some(brow);
                    brow_priority = priority;
                }
                if matches!(severity, StatSeverity::High) {
                    extras.tears = Some(TearConfig { eye: TearEye::Alternating });
                } else if matches!(severity, StatSeverity::Critical) {
                    extras.tears = Some(TearConfig { eye: TearEye::Both });
                }
            }
            _ => {}
        }
    }

    if avg > 85.0 {
        body_effects.push(BodyEffect::Sparkle);
    } else if stats.happiness > 95.0 {
        body_effects.push(BodyEffect::Love);
    }

    if matches!(eye_type, EyeType::Happy) && avg > 80.0 {
        eye_type = EyeType::Excited;
    }

    let animation = if avg > 80.0 {
        AnimationType::Excited
    } else if stats.lowest().1 < 25.0 {
        AnimationType::Sad
    } else {
        AnimationType::Idle
    };

    ComposableRecipe {
        eye_type,
        mouth_type,
        eyebrow,
        body_effects,
        extras,
        animation,
    }
}

pub fn stat_color(value: f64) -> &'static str {
    if value >= 70.0 {
        "#22c55e"
    } else if value >= 40.0 {
        "#eab308"
    } else if value >= 20.0 {
        "#f97316"
    } else {
        "#ef4444"
    }
}

#[allow(dead_code)]
pub fn attenuate_for_feed(recipe: &ComposableRecipe) -> ComposableRecipe {
    let mut r = recipe.clone();
    r.body_effects = r
        .body_effects
        .iter()
        .filter(|e| !matches!(e, BodyEffect::StinkFlies))
        .take(2)
        .cloned()
        .collect();
    r.extras.tears = None;
    r
}

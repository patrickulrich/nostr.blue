use crate::utils::nip_bb::BlobbiStats;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisualRecipe {
    pub eye_type: EyeType,
    pub mouth_type: MouthType,
    pub body_effect: BodyEffect,
    pub animation: AnimationType,
}

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
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum MouthType {
    #[default]
    Smile,
    Frown,
    Open,
    Sleeping,
    Neutral,
    Grin,
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
}

pub fn resolve_recipe(stats: &BlobbiStats, is_sleeping: bool, _mood: &str) -> VisualRecipe {
    if is_sleeping {
        return VisualRecipe {
            eye_type: EyeType::Sleeping,
            mouth_type: MouthType::Sleeping,
            body_effect: BodyEffect::Sleeping,
            animation: AnimationType::Sleep,
        };
    }

    let (lowest_stat, lowest_val) = stats.lowest();
    let avg = stats.average();

    let eye_type = if lowest_val < 20.0 {
        match lowest_stat {
            "hunger" => EyeType::Hungry,
            "happiness" => EyeType::Sad,
            "health" => EyeType::Tired,
            "hygiene" => EyeType::Bored,
            "energy" => EyeType::Tired,
            _ => EyeType::Sad,
        }
    } else if lowest_val < 40.0 {
        EyeType::Content
    } else if avg > 80.0 {
        EyeType::Excited
    } else {
        EyeType::Happy
    };

    let mouth_type = if lowest_val < 20.0 {
        MouthType::Frown
    } else if stats.happiness > 80.0 {
        MouthType::Grin
    } else if stats.hunger < 30.0 {
        MouthType::Open
    } else if avg > 60.0 {
        MouthType::Smile
    } else {
        MouthType::Neutral
    };

    let body_effect = if stats.hygiene < 25.0 {
        BodyEffect::Dirt
    } else if stats.hygiene < 40.0 {
        BodyEffect::Stink
    } else if avg > 85.0 {
        BodyEffect::Sparkle
    } else {
        BodyEffect::None
    };

    let animation = if avg > 80.0 {
        AnimationType::Excited
    } else if lowest_val < 25.0 {
        AnimationType::Sad
    } else {
        AnimationType::Idle
    };

    VisualRecipe {
        eye_type,
        mouth_type,
        body_effect,
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

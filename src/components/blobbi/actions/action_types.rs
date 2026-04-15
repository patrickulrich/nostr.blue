use serde::{Deserialize, Serialize};

use crate::utils::nip_bb::BlobbiStage;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobbiActionType {
    #[default]
    Feed,
    Play,
    Clean,
    Rest,
    Warm,
    Check,
    Sing,
    Talk,
    Medicine,
    Cruzar,
    UseItem,
}

impl BlobbiActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlobbiActionType::Feed => "feed",
            BlobbiActionType::Play => "play",
            BlobbiActionType::Clean => "clean",
            BlobbiActionType::Rest => "rest",
            BlobbiActionType::Warm => "warm",
            BlobbiActionType::Check => "check",
            BlobbiActionType::Sing => "sing",
            BlobbiActionType::Talk => "talk",
            BlobbiActionType::Medicine => "medicine",
            BlobbiActionType::Cruzar => "cruzar",
            BlobbiActionType::UseItem => "use_item",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "feed" => Some(BlobbiActionType::Feed),
            "play" => Some(BlobbiActionType::Play),
            "clean" => Some(BlobbiActionType::Clean),
            "rest" => Some(BlobbiActionType::Rest),
            "warm" => Some(BlobbiActionType::Warm),
            "check" => Some(BlobbiActionType::Check),
            "sing" => Some(BlobbiActionType::Sing),
            "talk" => Some(BlobbiActionType::Talk),
            "medicine" => Some(BlobbiActionType::Medicine),
            "cruzar" => Some(BlobbiActionType::Cruzar),
            _ => None,
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            BlobbiActionType::Feed => "care",
            BlobbiActionType::Play => "entertainment",
            BlobbiActionType::Clean => "hygiene",
            BlobbiActionType::Rest => "care",
            BlobbiActionType::Warm => "egg_care",
            BlobbiActionType::Check => "egg_care",
            BlobbiActionType::Sing => "entertainment",
            BlobbiActionType::Talk => "social",
            BlobbiActionType::Medicine => "health",
            BlobbiActionType::Cruzar => "breeding",
            BlobbiActionType::UseItem => "inventory",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            BlobbiActionType::Feed => "Feed",
            BlobbiActionType::Play => "Play",
            BlobbiActionType::Clean => "Clean",
            BlobbiActionType::Rest => "Rest",
            BlobbiActionType::Warm => "Warm",
            BlobbiActionType::Check => "Check",
            BlobbiActionType::Sing => "Sing",
            BlobbiActionType::Talk => "Talk",
            BlobbiActionType::Medicine => "Medicine",
            BlobbiActionType::Cruzar => "Cruzar",
            BlobbiActionType::UseItem => "Use Item",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            BlobbiActionType::Feed => "\u{1F354}",
            BlobbiActionType::Play => "\u{1F3AE}",
            BlobbiActionType::Clean => "\u{1F9F9}",
            BlobbiActionType::Rest => "\u{1F634}",
            BlobbiActionType::Warm => "\u{1F525}",
            BlobbiActionType::Check => "\u{1F50D}",
            BlobbiActionType::Sing => "\u{1F3A4}",
            BlobbiActionType::Talk => "\u{1F4AC}",
            BlobbiActionType::Medicine => "\u{1F48A}",
            BlobbiActionType::Cruzar => "\u{1F43E}",
            BlobbiActionType::UseItem => "\u{1F9EA}",
        }
    }

    pub fn stat_changes(&self) -> Vec<(&'static str, f64)> {
        match self {
            BlobbiActionType::Feed => vec![("hunger", 30.0), ("happiness", 5.0)],
            BlobbiActionType::Play => {
                vec![("happiness", 25.0), ("energy", -15.0), ("hygiene", -5.0)]
            }
            BlobbiActionType::Clean => vec![("hygiene", 40.0), ("happiness", 10.0)],
            BlobbiActionType::Rest => vec![("energy", 50.0), ("happiness", 5.0)],
            BlobbiActionType::Warm => vec![("health", 5.0), ("happiness", 2.0)],
            BlobbiActionType::Check => vec![("health", 2.0)],
            BlobbiActionType::Sing => vec![("happiness", 15.0), ("energy", -5.0)],
            BlobbiActionType::Talk => vec![("happiness", 10.0)],
            BlobbiActionType::Medicine => vec![("health", 30.0), ("happiness", -5.0)],
            BlobbiActionType::Cruzar => vec![("happiness", 20.0), ("energy", -10.0)],
            BlobbiActionType::UseItem => vec![],
        }
    }

    pub fn xp_value(&self) -> u64 {
        match self {
            BlobbiActionType::Play => 10,
            BlobbiActionType::Feed
            | BlobbiActionType::Clean
            | BlobbiActionType::Rest
            | BlobbiActionType::Warm
            | BlobbiActionType::Check
            | BlobbiActionType::Sing
            | BlobbiActionType::Talk
            | BlobbiActionType::Medicine
            | BlobbiActionType::Cruzar => 5,
            BlobbiActionType::UseItem => 3,
        }
    }

    pub fn is_care_action(&self) -> bool {
        matches!(
            self,
            BlobbiActionType::Feed
                | BlobbiActionType::Play
                | BlobbiActionType::Clean
                | BlobbiActionType::Medicine
        )
    }

    #[allow(dead_code)]
    pub fn available_for_stage(&self, stage: BlobbiStage) -> bool {
        match stage {
            BlobbiStage::Egg => matches!(
                self,
                BlobbiActionType::Warm
                    | BlobbiActionType::Check
                    | BlobbiActionType::Sing
                    | BlobbiActionType::Talk
            ),
            BlobbiStage::Baby => matches!(
                self,
                BlobbiActionType::Feed
                    | BlobbiActionType::Play
                    | BlobbiActionType::Clean
                    | BlobbiActionType::Rest
                    | BlobbiActionType::Medicine
                    | BlobbiActionType::Talk
                    | BlobbiActionType::Sing
            ),
            BlobbiStage::Adult => matches!(
                self,
                BlobbiActionType::Feed
                    | BlobbiActionType::Play
                    | BlobbiActionType::Clean
                    | BlobbiActionType::Rest
                    | BlobbiActionType::Talk
                    | BlobbiActionType::Sing
                    | BlobbiActionType::Cruzar
            ),
        }
    }

    #[allow(dead_code)]
    pub fn available_for_egg(&self) -> bool {
        self.available_for_stage(BlobbiStage::Egg)
    }

    #[allow(dead_code)]
    pub fn available_for_baby(&self) -> bool {
        self.available_for_stage(BlobbiStage::Baby)
    }

    #[allow(dead_code)]
    pub fn available_for_adult(&self) -> bool {
        self.available_for_stage(BlobbiStage::Adult)
    }
}

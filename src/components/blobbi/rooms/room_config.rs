#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlobbiRoomId {
    Care,
    Closet,
    Kitchen,
    #[default]
    Home,
    Hatchery,
    Rest,
}

impl BlobbiRoomId {
    pub fn label(&self) -> &'static str {
        match self {
            BlobbiRoomId::Care => "Care Room",
            BlobbiRoomId::Closet => "Wardrobe",
            BlobbiRoomId::Kitchen => "Kitchen",
            BlobbiRoomId::Home => "Home",
            BlobbiRoomId::Hatchery => "Hatchery",
            BlobbiRoomId::Rest => "Bedroom",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            BlobbiRoomId::Care => "\u{1FA79}",
            BlobbiRoomId::Closet => "\u{1F457}",
            BlobbiRoomId::Kitchen => "\u{1F373}",
            BlobbiRoomId::Home => "\u{1F3E0}",
            BlobbiRoomId::Hatchery => "\u{1F95A}",
            BlobbiRoomId::Rest => "\u{1F319}",
        }
    }

    #[allow(dead_code)]
    pub fn description(&self) -> &'static str {
        match self {
            BlobbiRoomId::Care => "Hygiene, care, and medicine",
            BlobbiRoomId::Closet => "Accessories and outfits",
            BlobbiRoomId::Kitchen => "Feed your Blobbi",
            BlobbiRoomId::Home => "Main living room",
            BlobbiRoomId::Hatchery => "Evolution and quests",
            BlobbiRoomId::Rest => "Rest and recharge",
        }
    }
}

pub const DEFAULT_ROOM_ORDER: &[BlobbiRoomId] =
    &[BlobbiRoomId::Care, BlobbiRoomId::Closet, BlobbiRoomId::Kitchen, BlobbiRoomId::Home, BlobbiRoomId::Hatchery, BlobbiRoomId::Rest];

#[allow(dead_code)]
pub const DEFAULT_INITIAL_ROOM: BlobbiRoomId = BlobbiRoomId::Home;

pub fn get_next_room(current: BlobbiRoomId) -> BlobbiRoomId {
    let idx = DEFAULT_ROOM_ORDER.iter().position(|&r| r == current).unwrap_or(0);
    DEFAULT_ROOM_ORDER[(idx + 1) % DEFAULT_ROOM_ORDER.len()]
}

pub fn get_previous_room(current: BlobbiRoomId) -> BlobbiRoomId {
    let idx = DEFAULT_ROOM_ORDER.iter().position(|&r| r == current).unwrap_or(0);
    DEFAULT_ROOM_ORDER[(idx + DEFAULT_ROOM_ORDER.len() - 1) % DEFAULT_ROOM_ORDER.len()]
}

#[allow(dead_code)]
pub fn get_room_index(current: BlobbiRoomId) -> usize {
    DEFAULT_ROOM_ORDER.iter().position(|&r| r == current).unwrap_or(0)
}

use crate::utils::nip_bb::BlobbiStage;

#[allow(dead_code)]
pub fn rooms_for_stage(stage: BlobbiStage) -> Vec<BlobbiRoomId> {
    match stage {
        BlobbiStage::Egg => vec![BlobbiRoomId::Home, BlobbiRoomId::Hatchery, BlobbiRoomId::Rest],
        BlobbiStage::Baby => DEFAULT_ROOM_ORDER.to_vec(),
        BlobbiStage::Adult => DEFAULT_ROOM_ORDER.to_vec(),
    }
}

#[allow(dead_code)]
pub fn room_label_for_stage(room: BlobbiRoomId, stage: BlobbiStage) -> &'static str {
    match stage {
        BlobbiStage::Egg => match room {
            BlobbiRoomId::Home => "Nest",
            BlobbiRoomId::Hatchery => "Hatch",
            BlobbiRoomId::Rest => "Warm",
            _ => room.label(),
        },
        _ => room.label(),
    }
}

#[allow(dead_code)]
pub fn is_room_available(room: BlobbiRoomId, stage: BlobbiStage) -> bool {
    rooms_for_stage(stage).contains(&room)
}

#![allow(dead_code)]

pub struct EyeOffsets {
    pub left_x: f64,
    pub right_x: f64,
    pub eye_y: f64,
    pub mouth_y: f64,
}

pub const BABY_EYES: EyeOffsets = EyeOffsets {
    left_x: 50.0,
    right_x: 90.0,
    eye_y: 60.0,
    mouth_y: 87.0,
};

pub const NOSTRICH_EYES: EyeOffsets = EyeOffsets {
    left_x: 68.0,
    right_x: 92.0,
    eye_y: 50.0,
    mouth_y: 108.0,
};

pub const PANDI_EYES: EyeOffsets = EyeOffsets {
    left_x: 55.0,
    right_x: 90.0,
    eye_y: 30.0,
    mouth_y: 100.0,
};

pub const OWLI_EYES: EyeOffsets = EyeOffsets {
    left_x: 50.0,
    right_x: 105.0,
    eye_y: 35.0,
    mouth_y: 108.0,
};

pub const CATTI_EYES: EyeOffsets = EyeOffsets {
    left_x: 60.0,
    right_x: 100.0,
    eye_y: 55.0,
    mouth_y: 105.0,
};

pub const FROGGI_EYES: EyeOffsets = EyeOffsets {
    left_x: 50.0,
    right_x: 105.0,
    eye_y: 35.0,
    mouth_y: 108.0,
};

pub const CLOUDI_EYES: EyeOffsets = EyeOffsets {
    left_x: 62.0,
    right_x: 98.0,
    eye_y: 62.0,
    mouth_y: 95.0,
};

pub const CRYSTI_EYES: EyeOffsets = EyeOffsets {
    left_x: 60.0,
    right_x: 100.0,
    eye_y: 55.0,
    mouth_y: 100.0,
};

pub const BLOOMI_EYES: EyeOffsets = EyeOffsets {
    left_x: 68.0,
    right_x: 92.0,
    eye_y: 72.0,
    mouth_y: 100.0,
};

pub const STARRI_EYES: EyeOffsets = EyeOffsets {
    left_x: 55.0,
    right_x: 90.0,
    eye_y: 30.0,
    mouth_y: 105.0,
};

pub const FLAMMI_EYES: EyeOffsets = EyeOffsets {
    left_x: 55.0,
    right_x: 90.0,
    eye_y: 30.0,
    mouth_y: 105.0,
};

pub const DROPPI_EYES: EyeOffsets = EyeOffsets {
    left_x: 68.0,
    right_x: 92.0,
    eye_y: 70.0,
    mouth_y: 95.0,
};

pub const BREEZY_EYES: EyeOffsets = EyeOffsets {
    left_x: 68.0,
    right_x: 92.0,
    eye_y: 60.0,
    mouth_y: 90.0,
};

pub const ROCKY_EYES: EyeOffsets = EyeOffsets {
    left_x: 55.0,
    right_x: 90.0,
    eye_y: 30.0,
    mouth_y: 105.0,
};

pub const CACTI_EYES: EyeOffsets = EyeOffsets {
    left_x: 70.0,
    right_x: 90.0,
    eye_y: 58.0,
    mouth_y: 95.0,
};

pub const MUSHIE_EYES: EyeOffsets = EyeOffsets {
    left_x: 68.0,
    right_x: 92.0,
    eye_y: 72.0,
    mouth_y: 108.0,
};

pub const LEAFY_EYES: EyeOffsets = EyeOffsets {
    left_x: 68.0,
    right_x: 92.0,
    eye_y: 68.0,
    mouth_y: 95.0,
};

pub const ROSEY_EYES: EyeOffsets = EyeOffsets {
    left_x: 68.0,
    right_x: 92.0,
    eye_y: 72.0,
    mouth_y: 100.0,
};

pub fn offsets_for_species(adult_type: &str) -> &'static EyeOffsets {
    match adult_type {
        "blobbi" => &NOSTRICH_EYES,
        "pandi" => &PANDI_EYES,
        "owli" => &OWLI_EYES,
        "catti" => &CATTI_EYES,
        "froggi" => &FROGGI_EYES,
        "cloudi" => &CLOUDI_EYES,
        "crysti" => &CRYSTI_EYES,
        "bloomi" => &BLOOMI_EYES,
        "starri" => &STARRI_EYES,
        "flammi" => &FLAMMI_EYES,
        "droppi" => &DROPPI_EYES,
        "breezy" => &BREEZY_EYES,
        "rocky" => &ROCKY_EYES,
        "cacti" => &CACTI_EYES,
        "mushie" => &MUSHIE_EYES,
        "leafy" => &LEAFY_EYES,
        "rosey" => &ROSEY_EYES,
        _ => &NOSTRICH_EYES,
    }
}

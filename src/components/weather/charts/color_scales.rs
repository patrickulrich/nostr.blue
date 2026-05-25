pub type ColorScale = Vec<(f64, (u8, u8, u8))>;

pub fn temperature_scale() -> ColorScale {
    vec![
        (-70.0, (115, 70, 105)),
        (-40.0, (82, 97, 178)),
        (0.0, (93, 133, 198)),
        (10.0, (43, 180, 150)),
        (20.0, (130, 210, 50)),
        (30.0, (232, 83, 25)),
        (40.0, (160, 22, 0)),
        (47.0, (71, 14, 0)),
    ]
}

pub fn uv_scale() -> ColorScale {
    vec![
        (0.0, (110, 110, 110)),
        (3.0, (76, 175, 80)),
        (6.0, (255, 235, 59)),
        (8.0, (255, 152, 0)),
        (11.0, (244, 67, 54)),
        (19.0, (200, 200, 200)),
    ]
}

pub fn humidity_scale() -> ColorScale {
    vec![
        (0.0, (173, 85, 56)),
        (50.0, (105, 173, 56)),
        (100.0, (56, 70, 114)),
    ]
}

pub fn wind_scale() -> ColorScale {
    vec![
        (0.0, (110, 183, 209)),
        (5.5, (54, 155, 212)),
        (10.8, (40, 125, 210)),
        (17.2, (50, 95, 200)),
        (24.5, (72, 64, 180)),
        (32.7, (100, 38, 150)),
        (50.0, (140, 18, 110)),
    ]
}

pub fn pressure_scale() -> ColorScale {
    vec![
        (900.0, (8, 16, 48)),
        (1013.25, (182, 182, 182)),
        (1080.0, (48, 8, 24)),
    ]
}

pub fn precipitation_scale() -> ColorScale {
    vec![(0.0, (100, 180, 220)), (20.0, (30, 80, 180))]
}

pub fn cloud_cover_scale() -> ColorScale {
    vec![
        (0.0, (146, 130, 70)),
        (50.0, (180, 175, 140)),
        (100.0, (213, 213, 205)),
    ]
}

pub fn color_at(scale: &ColorScale, value: f64) -> String {
    if scale.is_empty() {
        return "#888888".to_string();
    }
    if value <= scale[0].0 {
        let (_, rgb) = scale[0];
        return format!("rgb({},{},{})", rgb.0, rgb.1, rgb.2);
    }
    if value >= scale[scale.len() - 1].0 {
        let (_, rgb) = scale[scale.len() - 1];
        return format!("rgb({},{},{})", rgb.0, rgb.1, rgb.2);
    }
    for i in 0..scale.len() - 1 {
        let (v0, c0) = scale[i];
        let (v1, c1) = scale[i + 1];
        if value >= v0 && value <= v1 {
            let t = if (v1 - v0).abs() < f64::EPSILON {
                0.0
            } else {
                (value - v0) / (v1 - v0)
            };
            let r = (c0.0 as f64 + t * (c1.0 as f64 - c0.0 as f64)) as u8;
            let g = (c0.1 as f64 + t * (c1.1 as f64 - c0.1 as f64)) as u8;
            let b = (c0.2 as f64 + t * (c1.2 as f64 - c0.2 as f64)) as u8;
            return format!("rgb({},{},{})", r, g, b);
        }
    }
    "#888888".to_string()
}

#[derive(Clone, PartialEq)]
pub struct HorizontalLine {
    pub value: f64,
    pub label: String,
    pub color: String,
    pub dashed: bool,
}

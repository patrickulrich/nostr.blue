pub fn lighten(hex: &str, percent: u32) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return "#ffffff".to_string();
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    let f = percent as f32 / 100.0;
    let r = ((r as f32 + (255.0 - r as f32) * f).min(255.0)) as u8;
    let g = ((g as f32 + (255.0 - g as f32) * f).min(255.0)) as u8;
    let b = ((b as f32 + (255.0 - b as f32) * f).min(255.0)) as u8;
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

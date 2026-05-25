#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemperatureUnit {
    Celsius,
    #[default]
    Fahrenheit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindSpeedUnit {
    Ms,
    Kmh,
    #[default]
    Mph,
    Knots,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureUnit {
    Hpa,
    Mmhg,
    #[default]
    Inhg,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrecipitationUnit {
    Mm,
    #[default]
    Inch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceUnit {
    Km,
    #[default]
    Miles,
}

pub fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

pub fn ms_to_kmh(ms: f64) -> f64 {
    ms * 3.6
}

pub fn ms_to_mph(ms: f64) -> f64 {
    ms * 2.237
}

pub fn ms_to_knots(ms: f64) -> f64 {
    ms * 1.944
}

pub fn ms_to_beaufort(ms: f64) -> u8 {
    if ms < 0.5 {
        0
    } else if ms < 1.6 {
        1
    } else if ms < 3.4 {
        2
    } else if ms < 5.5 {
        3
    } else if ms < 8.0 {
        4
    } else if ms < 10.8 {
        5
    } else if ms < 13.9 {
        6
    } else if ms < 17.2 {
        7
    } else if ms < 20.8 {
        8
    } else if ms < 24.5 {
        9
    } else if ms < 28.5 {
        10
    } else if ms < 32.7 {
        11
    } else {
        12
    }
}

pub fn beaufort_description(bft: u8) -> &'static str {
    match bft {
        0 => "Calm",
        1 => "Light air",
        2 => "Light breeze",
        3 => "Gentle breeze",
        4 => "Moderate breeze",
        5 => "Fresh breeze",
        6 => "Strong breeze",
        7 => "Near gale",
        8 => "Gale",
        9 => "Strong gale",
        10 => "Storm",
        11 => "Violent storm",
        _ => "Hurricane",
    }
}

pub fn hpa_to_mmhg(hpa: f64) -> f64 {
    hpa * 0.750062
}

pub fn hpa_to_inhg(hpa: f64) -> f64 {
    hpa * 0.02953
}

pub fn mm_to_inches(mm: f64) -> f64 {
    mm / 25.4
}

pub fn km_to_miles(km: f64) -> f64 {
    km * 0.621371
}

pub fn meters_to_display(m: f64, unit: DistanceUnit) -> String {
    if m >= 1000.0 {
        let km = m / 1000.0;
        match unit {
            DistanceUnit::Km => format!("{:.0} km", km),
            DistanceUnit::Miles => format!("{:.0} mi", km_to_miles(km)),
        }
    } else {
        match unit {
            DistanceUnit::Km => format!("{:.0} m", m),
            DistanceUnit::Miles => format!("{:.0} m", m),
        }
    }
}

pub fn format_temperature(celsius: f64, unit: TemperatureUnit) -> String {
    match unit {
        TemperatureUnit::Celsius => format!("{:.0}\u{00B0}C", celsius),
        TemperatureUnit::Fahrenheit => format!("{:.0}\u{00B0}F", celsius_to_fahrenheit(celsius)),
    }
}

pub fn format_temperature_brief(celsius: f64, unit: TemperatureUnit) -> String {
    match unit {
        TemperatureUnit::Celsius => format!("{:.0}\u{00B0}", celsius),
        TemperatureUnit::Fahrenheit => format!("{:.0}\u{00B0}", celsius_to_fahrenheit(celsius)),
    }
}

pub fn format_wind_speed(ms: f64, unit: WindSpeedUnit) -> String {
    match unit {
        WindSpeedUnit::Ms => format!("{:.1} m/s", ms),
        WindSpeedUnit::Kmh => format!("{:.0} km/h", ms_to_kmh(ms)),
        WindSpeedUnit::Mph => format!("{:.0} mph", ms_to_mph(ms)),
        WindSpeedUnit::Knots => format!("{:.0} kn", ms_to_knots(ms)),
    }
}

#[allow(dead_code)]
pub fn format_pressure(hpa: f64, unit: PressureUnit) -> String {
    match unit {
        PressureUnit::Hpa => format!("{:.0} hPa", hpa),
        PressureUnit::Mmhg => format!("{:.0} mmHg", hpa_to_mmhg(hpa)),
        PressureUnit::Inhg => format!("{:.2} inHg", hpa_to_inhg(hpa)),
    }
}

pub fn format_precipitation(mm: f64, unit: PrecipitationUnit) -> String {
    match unit {
        PrecipitationUnit::Mm => format!("{:.1} mm", mm),
        PrecipitationUnit::Inch => format!("{:.2} in", mm_to_inches(mm)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AqiLevel {
    Good,
    Fair,
    Low,
    Moderate,
    Poor,
    VeryPoor,
    Extreme,
}

impl AqiLevel {
    pub fn from_aqi(aqi: f64) -> Self {
        if aqi <= 20.0 {
            Self::Good
        } else if aqi <= 50.0 {
            Self::Fair
        } else if aqi <= 80.0 {
            Self::Low
        } else if aqi <= 120.0 {
            Self::Moderate
        } else if aqi <= 200.0 {
            Self::Poor
        } else if aqi <= 350.0 {
            Self::VeryPoor
        } else {
            Self::Extreme
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Good => "Good",
            Self::Fair => "Fair",
            Self::Low => "Low",
            Self::Moderate => "Moderate",
            Self::Poor => "Poor",
            Self::VeryPoor => "Very Poor",
            Self::Extreme => "Extreme",
        }
    }

    pub fn color(&self) -> &'static str {
        match self {
            Self::Good => "#4CAF50",
            Self::Fair => "#8BC34A",
            Self::Low => "#FFEB3B",
            Self::Moderate => "#FF9800",
            Self::Poor => "#F44336",
            Self::VeryPoor => "#9C27B0",
            Self::Extreme => "#7E0023",
        }
    }
}

pub fn calculate_aqi(pm25: f64, pm10: f64, no2: f64, o3: f64, _so2: f64, _co: f64) -> f64 {
    let subs = [
        pm25_aqi_subindex(pm25),
        pm10_aqi_subindex(pm10),
        no2_aqi_subindex(no2),
        o3_aqi_subindex(o3),
    ];
    subs.into_iter().fold(0.0_f64, f64::max)
}

fn pm25_aqi_subindex(v: f64) -> f64 {
    if v <= 10.0 {
        v * 2.0
    } else if v <= 20.0 {
        20.0 + (v - 10.0) * 3.0
    } else if v <= 50.0 {
        50.0 + (v - 20.0) * (50.0 / 30.0)
    } else if v <= 120.0 {
        100.0 + (v - 50.0) * (100.0 / 70.0)
    } else {
        200.0 + (v - 120.0) * (200.0 / 130.0)
    }
}

fn pm10_aqi_subindex(v: f64) -> f64 {
    if v <= 20.0 {
        v * 2.5
    } else if v <= 50.0 {
        50.0 + (v - 20.0) * (50.0 / 30.0)
    } else if v <= 120.0 {
        100.0 + (v - 50.0) * (100.0 / 70.0)
    } else {
        200.0 + (v - 120.0) * (200.0 / 130.0)
    }
}

fn no2_aqi_subindex(v: f64) -> f64 {
    if v <= 40.0 {
        v * 1.25
    } else if v <= 100.0 {
        50.0 + (v - 40.0) * (50.0 / 60.0)
    } else {
        100.0 + (v - 100.0) * (100.0 / 150.0)
    }
}

fn o3_aqi_subindex(v: f64) -> f64 {
    if v <= 60.0 {
        v * (50.0 / 60.0)
    } else if v <= 120.0 {
        50.0 + (v - 60.0) * (50.0 / 60.0)
    } else {
        100.0 + (v - 120.0) * (100.0 / 80.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UvLevel {
    Low,
    Moderate,
    High,
    VeryHigh,
    Extreme,
}

impl UvLevel {
    pub fn from_index(uv: f64) -> Self {
        if uv <= 2.0 {
            Self::Low
        } else if uv <= 5.0 {
            Self::Moderate
        } else if uv <= 7.0 {
            Self::High
        } else if uv <= 10.0 {
            Self::VeryHigh
        } else {
            Self::Extreme
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Moderate => "Moderate",
            Self::High => "High",
            Self::VeryHigh => "Very High",
            Self::Extreme => "Extreme",
        }
    }

    pub fn color(&self) -> &'static str {
        match self {
            Self::Low => "#4CAF50",
            Self::Moderate => "#FFEB3B",
            Self::High => "#FF9800",
            Self::VeryHigh => "#F44336",
            Self::Extreme => "#9C27B0",
        }
    }
}

pub fn visibility_description(m: f64) -> &'static str {
    if m < 50.0 {
        "Dense fog"
    } else if m < 200.0 {
        "Thick fog"
    } else if m < 1000.0 {
        "Fog"
    } else if m < 2000.0 {
        "Mist"
    } else if m < 4000.0 {
        "Poor visibility"
    } else if m < 10000.0 {
        "Moderate visibility"
    } else if m < 20000.0 {
        "Good visibility"
    } else {
        "Excellent visibility"
    }
}

pub fn humidity_description(h: i32) -> &'static str {
    if h < 25 {
        "Very dry"
    } else if h < 40 {
        "Dry"
    } else if h < 60 {
        "Comfortable"
    } else if h < 75 {
        "Humid"
    } else {
        "Very humid"
    }
}

pub fn pressure_trend(hourly_pressure: &[f64]) -> &'static str {
    if hourly_pressure.len() < 3 {
        return "";
    }
    let recent = &hourly_pressure[hourly_pressure.len() - 3..];
    let diff = recent[2] - recent[0];
    if diff > 0.5 {
        "\u{2191} Rising"
    } else if diff < -0.5 {
        "\u{2193} Falling"
    } else {
        "\u{2192} Steady"
    }
}

pub fn moon_phase_name(phase: f64) -> &'static str {
    let normalized = phase.rem_euclid(360.0);
    if !(22.5..337.5).contains(&normalized) {
        "New Moon"
    } else if normalized < 67.5 {
        "Waxing Crescent"
    } else if normalized < 112.5 {
        "First Quarter"
    } else if normalized < 157.5 {
        "Waxing Gibbous"
    } else if normalized < 202.5 {
        "Full Moon"
    } else if normalized < 247.5 {
        "Waning Gibbous"
    } else if normalized < 292.5 {
        "Last Quarter"
    } else {
        "Waning Crescent"
    }
}

pub fn moon_emoji(phase: f64) -> &'static str {
    let normalized = phase.rem_euclid(360.0);
    if !(22.5..337.5).contains(&normalized) {
        "\u{1F311}"
    } else if normalized < 67.5 {
        "\u{1F312}"
    } else if normalized < 112.5 {
        "\u{1F313}"
    } else if normalized < 157.5 {
        "\u{1F314}"
    } else if normalized < 202.5 {
        "\u{1F315}"
    } else if normalized < 247.5 {
        "\u{1F316}"
    } else if normalized < 292.5 {
        "\u{1F317}"
    } else {
        "\u{1F318}"
    }
}

pub fn wind_direction_label(degrees: i32) -> &'static str {
    let d = degrees.rem_euclid(360);
    match d {
        0..=11 | 349..=360 => "N",
        12..=33 => "NNE",
        34..=56 => "NE",
        57..=78 => "ENE",
        79..=101 => "E",
        102..=123 => "ESE",
        124..=146 => "SE",
        147..=168 => "SSE",
        169..=191 => "S",
        192..=213 => "SSW",
        214..=236 => "SW",
        237..=258 => "WSW",
        259..=281 => "W",
        282..=303 => "WNW",
        304..=326 => "NW",
        327..=348 => "NNW",
        _ => "?",
    }
}

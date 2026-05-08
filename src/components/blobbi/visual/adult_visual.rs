use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::baby_visual::render_body_effects;
use crate::components::blobbi::visual::recipe::*;

#[component]
pub fn AdultVisual(blobbi: BlobbiCompanion, recipe: ComposableRecipe) -> Element {
    let base_color = &blobbi.visual_traits.base_color;
    let eye_color = &blobbi.visual_traits.eye_color;
    let adult_type = blobbi.adult_type.as_deref().unwrap_or("blobbi");

    let animation_class = match recipe.animation {
        AnimationType::Excited => "animate-[blobbi-idle-bounce_1.5s_ease-in-out_infinite]",
        AnimationType::Sad => "animate-[blobbi-sad-breathe_3s_ease-in-out_infinite]",
        _ => "animate-[blobbi-idle-bounce_3s_ease-in-out_infinite]",
    };

    let bc = base_color.clone();
    let ec = eye_color.clone();
    let ac = animation_class.to_string();

    match adult_type {
        "blobbi" => {
            rsx! { NostrichVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "pandi" => {
            rsx! { PandiVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "owli" => {
            rsx! { OwliVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "catti" => {
            rsx! { CattiVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "froggi" => {
            rsx! { FroggiVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "cloudi" => {
            rsx! { CloudiVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "crysti" => {
            rsx! { CrystiVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "bloomi" => {
            rsx! { BloomiVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "starri" => {
            rsx! { StarriVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "flammi" => {
            rsx! { FlammiVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "droppi" => {
            rsx! { DroppiVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "breezy" => {
            rsx! { BreezyVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "rocky" => {
            rsx! { RockyVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "cacti" => {
            rsx! { CactiVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "mushie" => {
            rsx! { MushieVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "leafy" => {
            rsx! { LeafyVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        "rosey" => {
            rsx! { RoseyVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } }
        }
        _ => rsx! { NostrichVisual { base_color: bc, eye_color: ec, recipe, animation_class: ac } },
    }
}

fn base_svg(
    animation_class: &str,
    gradient_id: &str,
    base_color: &str,
    body: Element,
    extras: Element,
) -> Element {
    rsx! {
        svg {
            class: "{animation_class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 160 160",
            width: "160",
            height: "160",
            defs {
                radialGradient { id: "{gradient_id}", cx: "40%", cy: "35%", r: "65%",
                    stop { offset: "0%", stop_color: "{lighten(base_color, 15)}" }
                    stop { offset: "100%", stop_color: "{base_color}" }
                }
            }
            {body}
            {extras}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_species(
    animation_class: &str,
    gradient_id: &str,
    base_color: &str,
    body: Element,
    face: Element,
    recipe: &ComposableRecipe,
    left_x: f64,
    right_x: f64,
    eye_y: f64,
    mouth_y: f64,
) -> Element {
    let brow_y = eye_y - 12.0;
    let brow = render_adult_eyebrow(&recipe.eyebrow, left_x, right_x, brow_y);
    let ext = render_adult_extras(&recipe.extras, left_x, right_x, eye_y, mouth_y);
    let effect = render_body_effects(&recipe.body_effects);
    let all_extras = rsx! {
        {face}
        {brow}
        {ext}
        {effect}
    };
    base_svg(animation_class, gradient_id, base_color, body, all_extras)
}
#[component]
fn NostrichVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 108.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 68.0, 92.0, 50.0);
    render_species(
        &animation_class,
        "nostrich-body",
        &base_color,
        rsx! {
            ellipse { cx: "80", cy: "120", rx: "32", ry: "28", fill: "url(#nostrich-body)" }
            path { d: "M 72 95 Q 70 75 72 55 L 88 55 Q 90 75 88 95", fill: "url(#nostrich-body)" }
            circle { cx: "80", cy: "45", r: "18", fill: "url(#nostrich-body)" }
            path { d: "M 96 42 L 110 47 L 96 50 Z", fill: "#f59e0b" }
            line { x1: "70", y1: "145", x2: "70", y2: "128", stroke: "#f59e0b", stroke_width: "3", stroke_linecap: "round" }
            line { x1: "90", y1: "145", x2: "90", y2: "128", stroke: "#f59e0b", stroke_width: "3", stroke_linecap: "round" }
            path { d: "M 62 145 L 70 145 L 75 148", fill: "none", stroke: "#f59e0b", stroke_width: "2", stroke_linecap: "round" }
            path { d: "M 82 148 L 90 145 L 98 145", fill: "none", stroke: "#f59e0b", stroke_width: "2", stroke_linecap: "round" }
            path { d: "M 110 115 Q 120 105 115 95 Q 125 100 120 110 Q 130 108 122 118", fill: "{lighten(&base_color, 10)}" }
            path { d: "M 60 100 Q 48 105 52 120 Q 55 115 62 115", fill: "{lighten(&base_color, 8)}", opacity: "0.7" }
        },
        rsx! {
            {eyes}
            {mouth}
        },
        &recipe,
        68.0, 92.0, 50.0, 108.0,
    )
}

// ── PANDI ───────────────────────────────────────────────────────
#[component]
fn PandiVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 100.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 55.0, 90.0, 30.0);
    render_species(
        &animation_class,
        "pandi-body",
        &base_color,
        rsx! {
            ellipse { cx: "80", cy: "95", rx: "55", ry: "48", fill: "url(#pandi-body)" }
            circle { cx: "45", cy: "60", r: "12", fill: "{base_color}", opacity: "0.7" }
            circle { cx: "115", cy: "60", r: "12", fill: "{base_color}", opacity: "0.7" }
            circle { cx: "45", cy: "58", r: "8", fill: "{lighten(&base_color, 10)}" }
            circle { cx: "115", cy: "58", r: "8", fill: "{lighten(&base_color, 10)}" }
        },
        rsx! {
            {eyes}
            {mouth}
            ellipse { cx: "50", cy: "95", rx: "10", ry: "6", fill: "rgba(255,150,150,0.25)" }
            ellipse { cx: "110", cy: "95", rx: "10", ry: "6", fill: "rgba(255,150,150,0.25)" }
        },
        &recipe,
        55.0, 90.0, 30.0, 100.0,
    )
}

// ── OWLI ────────────────────────────────────────────────────────
#[component]
fn OwliVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 108.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 50.0, 105.0, 35.0);
    render_species(
        &animation_class,
        "owli-body",
        &base_color,
        rsx! {
            ellipse { cx: "80", cy: "95", rx: "45", ry: "50", fill: "url(#owli-body)" }
            path { d: "M 35 85 Q 25 65 40 55 L 55 75 Z", fill: "{lighten(&base_color, 10)}" }
            path { d: "M 125 85 Q 135 65 120 55 L 105 75 Z", fill: "{lighten(&base_color, 10)}" }
            circle { cx: "55", cy: "70", r: "16", fill: "white" }
            circle { cx: "105", cy: "70", r: "16", fill: "white" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        50.0, 105.0, 35.0, 108.0,
    )
}

// ── CATTI ───────────────────────────────────────────────────────
#[component]
fn CattiVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 105.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 60.0, 100.0, 55.0);
    render_species(
        &animation_class,
        "catti-body",
        &base_color,
        rsx! {
            ellipse { cx: "80", cy: "100", rx: "40", ry: "42", fill: "url(#catti-body)" }
            circle { cx: "80", cy: "58", r: "28", fill: "url(#catti-body)" }
            polygon { points: "55,38 48,12 68,32", fill: "{base_color}" }
            polygon { points: "105,38 112,12 92,32", fill: "{base_color}" }
            polygon { points: "57,36 52,18 66,33", fill: "{lighten(&base_color, 15)}" }
            polygon { points: "103,36 108,18 94,33", fill: "{lighten(&base_color, 15)}" }
            path { d: "M 118 95 Q 140 80 135 65 Q 145 70 140 85 Q 148 78 142 95", fill: "{base_color}" }
            line { x1: "42", y1: "62", x2: "20", y2: "58", stroke: "currentColor", stroke_width: "1", class: "text-foreground/30" }
            line { x1: "42", y1: "66", x2: "20", y2: "68", stroke: "currentColor", stroke_width: "1", class: "text-foreground/30" }
            line { x1: "118", y1: "62", x2: "140", y2: "58", stroke: "currentColor", stroke_width: "1", class: "text-foreground/30" }
            line { x1: "118", y1: "66", x2: "140", y2: "68", stroke: "currentColor", stroke_width: "1", class: "text-foreground/30" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        60.0, 100.0, 55.0, 105.0,
    )
}

// ── FROGGI ──────────────────────────────────────────────────────
#[component]
fn FroggiVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 108.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 50.0, 105.0, 35.0);
    render_species(
        &animation_class,
        "froggi-body",
        &base_color,
        rsx! {
            ellipse { cx: "80", cy: "100", rx: "58", ry: "40", fill: "url(#froggi-body)" }
            circle { cx: "50", cy: "55", r: "18", fill: "{base_color}" }
            circle { cx: "110", cy: "55", r: "18", fill: "{base_color}" }
            circle { cx: "50", cy: "53", r: "14", fill: "white" }
            circle { cx: "110", cy: "53", r: "14", fill: "white" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        50.0, 105.0, 35.0, 108.0,
    )
}

// ── CLOUDI ──────────────────────────────────────────────────────
#[component]
fn CloudiVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 95.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 62.0, 98.0, 62.0);
    render_species(
        &animation_class,
        "cloudi-body",
        &base_color,
        rsx! {
            circle { cx: "55", cy: "80", r: "30", fill: "url(#cloudi-body)" }
            circle { cx: "105", cy: "80", r: "30", fill: "url(#cloudi-body)" }
            circle { cx: "80", cy: "65", r: "35", fill: "url(#cloudi-body)" }
            rect { x: "25", y: "90", width: "110", height: "25", rx: "12", fill: "url(#cloudi-body)" }
            circle { cx: "35", cy: "95", r: "15", fill: "{lighten(&base_color, 10)}", opacity: "0.4" }
            circle { cx: "125", cy: "95", r: "12", fill: "{lighten(&base_color, 10)}", opacity: "0.4" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        62.0, 98.0, 62.0, 95.0,
    )
}

// ── CRYSTI ──────────────────────────────────────────────────────
#[component]
fn CrystiVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 100.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 60.0, 100.0, 55.0);
    render_species(
        &animation_class,
        "crysti-body",
        &base_color,
        rsx! {
            polygon { points: "80,20 110,55 100,120 60,120 50,55", fill: "url(#crysti-body)" }
            polygon { points: "80,20 110,55 80,65", fill: "{lighten(&base_color, 20)}", opacity: "0.5" }
            polygon { points: "80,20 50,55 80,65", fill: "{lighten(&base_color, 10)}", opacity: "0.3" }
            polygon { points: "80,65 100,120 80,120", fill: "{lighten(&base_color, 5)}", opacity: "0.3" }
            polygon { points: "45,60 30,90 50,100", fill: "{lighten(&base_color, 12)}", opacity: "0.6" }
            polygon { points: "115,60 130,90 110,100", fill: "{lighten(&base_color, 12)}", opacity: "0.6" }
            line { x1: "75", y1: "30", x2: "72", y2: "50", stroke: "rgba(255,255,255,0.5)", stroke_width: "2", stroke_linecap: "round" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        60.0, 100.0, 55.0, 100.0,
    )
}

// ── BLOOMI ──────────────────────────────────────────────────────
#[component]
fn BloomiVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 100.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 68.0, 92.0, 72.0);
    render_species(
        &animation_class,
        "bloomi-body",
        &base_color,
        rsx! {
            path { d: "M 80 130 Q 80 110 80 95", fill: "none", stroke: "#22c55e", stroke_width: "4", stroke_linecap: "round" }
            path { d: "M 80 115 Q 65 108 60 118 Q 68 115 80 115", fill: "#22c55e" }
            path { d: "M 80 110 Q 95 103 100 113 Q 92 110 80 110", fill: "#22c55e" }
            ellipse { cx: "80", cy: "55", rx: "18", ry: "28", fill: "url(#bloomi-body)" }
            ellipse { cx: "80", cy: "55", rx: "18", ry: "28", fill: "url(#bloomi-body)", transform: "rotate(60 80 80)" }
            ellipse { cx: "80", cy: "55", rx: "18", ry: "28", fill: "url(#bloomi-body)", transform: "rotate(120 80 80)" }
            ellipse { cx: "80", cy: "55", rx: "18", ry: "28", fill: "url(#bloomi-body)", transform: "rotate(180 80 80)" }
            ellipse { cx: "80", cy: "55", rx: "18", ry: "28", fill: "url(#bloomi-body)", transform: "rotate(240 80 80)" }
            ellipse { cx: "80", cy: "55", rx: "18", ry: "28", fill: "url(#bloomi-body)", transform: "rotate(300 80 80)" }
            circle { cx: "80", cy: "80", r: "18", fill: "{lighten(&base_color, 25)}" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        68.0, 92.0, 72.0, 100.0,
    )
}

// ── STARRI ──────────────────────────────────────────────────────
#[component]
fn StarriVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 105.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 55.0, 90.0, 30.0);
    render_species(
        &animation_class,
        "starri-body",
        &base_color,
        rsx! {
            polygon { points: "80,25 90,60 127,60 97,82 108,118 80,97 52,118 63,82 33,60 70,60", fill: "url(#starri-body)" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        55.0, 90.0, 30.0, 105.0,
    )
}

// ── FLAMMI ──────────────────────────────────────────────────────
#[component]
fn FlammiVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 105.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 55.0, 90.0, 30.0);
    render_species(
        &animation_class,
        "flammi-body",
        &base_color,
        rsx! {
            path { d: "M 35 120 Q 30 80 50 60 Q 55 35 65 50 Q 80 25 90 45 Q 95 30 100 50 Q 120 60 125 120 Z", fill: "url(#flammi-body)" }
            path { d: "M 50 55 Q 55 40 65 50 Q 75 35 85 48", fill: "{lighten(&base_color, 20)}", opacity: "0.5" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        55.0, 90.0, 30.0, 105.0,
    )
}

// ── DROPPi ──────────────────────────────────────────────────────
#[component]
fn DroppiVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 95.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 68.0, 92.0, 70.0);
    render_species(
        &animation_class,
        "droppi-body",
        &base_color,
        rsx! {
            path { d: "M 80 25 Q 45 70 45 95 Q 45 130 80 135 Q 115 130 115 95 Q 115 70 80 25 Z", fill: "url(#droppi-body)" }
            path { d: "M 72 45 Q 62 70 60 85 Q 58 95 65 90 Q 68 70 72 45 Z", fill: "rgba(255,255,255,0.3)" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        68.0, 92.0, 70.0, 95.0,
    )
}

// ── BREEZY ──────────────────────────────────────────────────────
#[component]
fn BreezyVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 90.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 68.0, 92.0, 60.0);
    render_species(
        &animation_class,
        "breezy-body",
        &base_color,
        rsx! {
            path { d: "M 50 80 Q 35 65 45 50 Q 55 35 75 40 Q 90 30 105 45 Q 120 55 115 75 Q 125 85 115 100 Q 110 120 90 115 Q 75 125 60 115 Q 40 110 50 80 Z", fill: "url(#breezy-body)" }
            path { d: "M 30 95 Q 20 90 25 80", fill: "none", stroke: "{lighten(&base_color, 20)}", stroke_width: "2", stroke_linecap: "round", opacity: "0.6" }
            path { d: "M 25 105 Q 15 100 20 88", fill: "none", stroke: "{lighten(&base_color, 20)}", stroke_width: "1.5", stroke_linecap: "round", opacity: "0.4" }
            path { d: "M 130 85 Q 140 80 135 70", fill: "none", stroke: "{lighten(&base_color, 20)}", stroke_width: "2", stroke_linecap: "round", opacity: "0.6" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        68.0, 92.0, 60.0, 90.0,
    )
}

// ── ROCKY ───────────────────────────────────────────────────────
#[component]
fn RockyVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 105.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 55.0, 90.0, 30.0);
    render_species(
        &animation_class,
        "rocky-body",
        &base_color,
        rsx! {
            polygon { points: "30,120 55,55 80,45 105,55 130,120", fill: "url(#rocky-body)" }
            polygon { points: "35,70 50,80 42,95", fill: "{lighten(&base_color, 8)}", opacity: "0.5" }
            polygon { points: "110,75 125,85 118,100", fill: "{lighten(&base_color, 8)}", opacity: "0.5" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        55.0, 90.0, 30.0, 105.0,
    )
}

// ── CACTI ───────────────────────────────────────────────────────
#[component]
fn CactiVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 95.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 70.0, 90.0, 58.0);
    render_species(
        &animation_class,
        "cacti-body",
        &base_color,
        rsx! {
            rect { x: "62", y: "40", width: "36", height: "80", rx: "18", fill: "url(#cacti-body)" }
            rect { x: "30", y: "65", width: "35", height: "18", rx: "9", fill: "url(#cacti-body)" }
            rect { x: "30", y: "55", width: "18", height: "28", rx: "9", fill: "url(#cacti-body)" }
            rect { x: "95", y: "55", width: "35", height: "18", rx: "9", fill: "url(#cacti-body)" }
            rect { x: "112", y: "45", width: "18", height: "28", rx: "9", fill: "url(#cacti-body)" }
            path { d: "M 50 120 L 110 120 L 105 145 L 55 145 Z", fill: "#a0522d" }
            line { x1: "60", y1: "60", x2: "55", y2: "55", stroke: "currentColor", stroke_width: "1", class: "text-foreground/30" }
            line { x1: "100", y1: "50", x2: "105", y2: "45", stroke: "currentColor", stroke_width: "1", class: "text-foreground/30" }
            line { x1: "60", y1: "85", x2: "54", y2: "83", stroke: "currentColor", stroke_width: "1", class: "text-foreground/30" }
            line { x1: "100", y1: "80", x2: "106", y2: "78", stroke: "currentColor", stroke_width: "1", class: "text-foreground/30" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        70.0, 90.0, 58.0, 95.0,
    )
}

// ── MUSHIE ──────────────────────────────────────────────────────
#[component]
fn MushieVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 108.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 68.0, 92.0, 72.0);
    render_species(
        &animation_class,
        "mushie-body",
        &base_color,
        rsx! {
            rect { x: "65", y: "95", width: "30", height: "40", rx: "8", fill: "#fef3c7" }
            ellipse { cx: "80", cy: "75", rx: "50", ry: "35", fill: "url(#mushie-body)" }
            circle { cx: "60", cy: "65", r: "8", fill: "rgba(255,255,255,0.4)" }
            circle { cx: "95", cy: "60", r: "6", fill: "rgba(255,255,255,0.4)" }
            circle { cx: "75", cy: "50", r: "5", fill: "rgba(255,255,255,0.3)" }
            circle { cx: "100", cy: "75", r: "4", fill: "rgba(255,255,255,0.3)" }
            path { d: "M 40 80 Q 60 95 80 95", fill: "none", stroke: "{lighten(&base_color, 10)}", stroke_width: "1", opacity: "0.4" }
            path { d: "M 120 80 Q 100 95 80 95", fill: "none", stroke: "{lighten(&base_color, 10)}", stroke_width: "1", opacity: "0.4" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        68.0, 92.0, 72.0, 108.0,
    )
}

// ── LEAFY ───────────────────────────────────────────────────────
#[component]
fn LeafyVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 95.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 68.0, 92.0, 68.0);
    render_species(
        &animation_class,
        "leafy-body",
        &base_color,
        rsx! {
            path { d: "M 80 25 Q 35 60 40 100 Q 50 135 80 140 Q 110 135 120 100 Q 125 60 80 25 Z", fill: "url(#leafy-body)" }
            line { x1: "80", y1: "35", x2: "80", y2: "130", stroke: "{lighten(&base_color, 15)}", stroke_width: "2", opacity: "0.5" }
            path { d: "M 80 55 Q 60 60 52 72", fill: "none", stroke: "{lighten(&base_color, 15)}", stroke_width: "1.5", opacity: "0.4" }
            path { d: "M 80 55 Q 100 60 108 72", fill: "none", stroke: "{lighten(&base_color, 15)}", stroke_width: "1.5", opacity: "0.4" }
            path { d: "M 80 80 Q 55 85 48 98", fill: "none", stroke: "{lighten(&base_color, 15)}", stroke_width: "1.5", opacity: "0.4" }
            path { d: "M 80 80 Q 105 85 112 98", fill: "none", stroke: "{lighten(&base_color, 15)}", stroke_width: "1.5", opacity: "0.4" }
            path { d: "M 80 105 Q 60 110 55 120", fill: "none", stroke: "{lighten(&base_color, 15)}", stroke_width: "1.5", opacity: "0.4" }
            path { d: "M 80 105 Q 100 110 105 120", fill: "none", stroke: "{lighten(&base_color, 15)}", stroke_width: "1.5", opacity: "0.4" }
            line { x1: "80", y1: "25", x2: "80", y2: "12", stroke: "#22c55e", stroke_width: "3", stroke_linecap: "round" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        68.0, 92.0, 68.0, 95.0,
    )
}

// ── ROSEY ───────────────────────────────────────────────────────
#[component]
fn RoseyVisual(
    base_color: String,
    eye_color: String,
    recipe: ComposableRecipe,
    animation_class: String,
) -> Element {
    let mouth = render_adult_mouth(recipe.mouth_type, 80.0, 100.0);
    let eyes = render_adult_eyes(recipe.eye_type, &eye_color, 68.0, 92.0, 72.0);
    render_species(
        &animation_class,
        "rosey-body",
        &base_color,
        rsx! {
            line { x1: "80", y1: "105", x2: "80", y2: "145", stroke: "#22c55e", stroke_width: "4", stroke_linecap: "round" }
            path { d: "M 80 120 L 74 115 L 80 118", fill: "#22c55e" }
            path { d: "M 80 132 L 86 127 L 80 130", fill: "#22c55e" }
            path { d: "M 80 125 Q 65 118 60 128 Q 68 122 80 125", fill: "#22c55e" }
            path { d: "M 80 40 Q 40 55 45 85 Q 55 75 65 80 Q 50 95 60 110 Q 70 95 80 100", fill: "url(#rosey-body)" }
            path { d: "M 80 40 Q 120 55 115 85 Q 105 75 95 80 Q 110 95 100 110 Q 90 95 80 100", fill: "url(#rosey-body)" }
            path { d: "M 80 50 Q 55 65 60 85 Q 70 75 80 80", fill: "{lighten(&base_color, 10)}" }
            path { d: "M 80 50 Q 105 65 100 85 Q 90 75 80 80", fill: "{lighten(&base_color, 10)}" }
            circle { cx: "80", cy: "80", r: "10", fill: "{lighten(&base_color, 25)}" }
            path { d: "M 80 72 Q 86 76 84 82 Q 80 86 76 82 Q 74 78 80 72 Z", fill: "{lighten(&base_color, 30)}" }
        },
        rsx! { {eyes} {mouth} },
        &recipe,
        68.0, 92.0, 72.0, 100.0,
    )
}

// ── Shared rendering helpers ────────────────────────────────────

fn render_adult_eyes(
    eye_type: EyeType,
    eye_color: &str,
    left_x: f64,
    right_x: f64,
    y: f64,
) -> Element {
    match eye_type {
        EyeType::Happy => rsx! {
            path { d: "M {left_x - 7.0} {y} Q {left_x} {y - 6.0} {left_x + 7.0} {y}", fill: "none", stroke: "{eye_color}", stroke_width: "3", stroke_linecap: "round" }
            path { d: "M {right_x - 7.0} {y} Q {right_x} {y - 6.0} {right_x + 7.0} {y}", fill: "none", stroke: "{eye_color}", stroke_width: "3", stroke_linecap: "round" }
        },
        EyeType::Sleeping => rsx! {
            path { d: "M {left_x - 7.0} {y} Q {left_x} {y - 3.0} {left_x + 7.0} {y}", fill: "none", stroke: "{eye_color}", stroke_width: "2.5", stroke_linecap: "round" }
            path { d: "M {right_x - 7.0} {y} Q {right_x} {y - 3.0} {right_x + 7.0} {y}", fill: "none", stroke: "{eye_color}", stroke_width: "2.5", stroke_linecap: "round" }
        },
        EyeType::Excited => rsx! {
            circle { cx: "{left_x}", cy: "{y}", r: "7", fill: "{eye_color}" }
            circle { cx: "{left_x - 2.0}", cy: "{y - 2.0}", r: "2.5", fill: "white" }
            circle { cx: "{right_x}", cy: "{y}", r: "7", fill: "{eye_color}" }
            circle { cx: "{right_x - 2.0}", cy: "{y - 2.0}", r: "2.5", fill: "white" }
        },
        EyeType::Watery => rsx! {
            circle { cx: "{left_x}", cy: "{y}", r: "7", fill: "{eye_color}" }
            circle { cx: "{left_x - 1.8}", cy: "{y - 2.1}", r: "2.8", fill: "white", opacity: "0.9" }
            circle { cx: "{left_x + 1.0}", cy: "{y + 2.5}", r: "1.8", fill: "white", opacity: "0.8" }
            path {
                d: "M {left_x - 6.0} {y + 1.5} Q {left_x - 6.0} {y + 6.0} {left_x} {y + 6.0} Q {left_x + 6.0} {y + 6.0} {left_x + 6.0} {y + 1.5} Z",
                fill: "#7dd3fc", opacity: "0.4",
            }
            circle { cx: "{right_x}", cy: "{y}", r: "7", fill: "{eye_color}" }
            circle { cx: "{right_x - 1.8}", cy: "{y - 2.1}", r: "2.8", fill: "white", opacity: "0.9" }
            circle { cx: "{right_x + 1.0}", cy: "{y + 2.5}", r: "1.8", fill: "white", opacity: "0.8" }
            path {
                d: "M {right_x - 6.0} {y + 1.5} Q {right_x - 6.0} {y + 6.0} {right_x} {y + 6.0} Q {right_x + 6.0} {y + 6.0} {right_x + 6.0} {y + 1.5} Z",
                fill: "#7dd3fc", opacity: "0.4",
            }
        },
        EyeType::Dizzy => rsx! {
            g {
                path {
                    d: "M {left_x} {y - 1.0} Q {left_x + 5.0} {y - 5.0} {left_x + 3.0} {y - 7.0} Q {left_x} {y - 9.0} {left_x - 3.0} {y - 5.0} Q {left_x - 5.0} {y} {left_x - 1.0} {y + 3.0} Q {left_x + 3.0} {y + 6.0} {left_x + 6.0} {y + 2.0}",
                    fill: "none", stroke: "{eye_color}", stroke_width: "2", stroke_linecap: "round",
                }
                animateTransform {
                    attribute_name: "transform",
                    type: "rotate",
                    from: "360 {left_x} {y}",
                    to: "0 {left_x} {y}",
                    dur: "2s",
                    repeat_count: "indefinite",
                }
            }
            g {
                path {
                    d: "M {right_x} {y - 1.0} Q {right_x + 5.0} {y - 5.0} {right_x + 3.0} {y - 7.0} Q {right_x} {y - 9.0} {right_x - 3.0} {y - 5.0} Q {right_x - 5.0} {y} {right_x - 1.0} {y + 3.0} Q {right_x + 3.0} {y + 6.0} {right_x + 6.0} {y + 2.0}",
                    fill: "none", stroke: "{eye_color}", stroke_width: "2", stroke_linecap: "round",
                }
                animateTransform {
                    attribute_name: "transform",
                    type: "rotate",
                    from: "360 {right_x} {y}",
                    to: "0 {right_x} {y}",
                    dur: "2s",
                    repeat_count: "indefinite",
                }
            }
        },
        EyeType::SleepyBlink => rsx! {
            path { d: "M {left_x - 7.0} {y} Q {left_x} {y - 3.0} {left_x + 7.0} {y}", fill: "none", stroke: "{eye_color}", stroke_width: "3", stroke_linecap: "round" }
            path { d: "M {right_x - 7.0} {y} Q {right_x} {y - 3.0} {right_x + 7.0} {y}", fill: "none", stroke: "{eye_color}", stroke_width: "3", stroke_linecap: "round" }
        },
        EyeType::Star => rsx! {
            polygon {
                points: "{left_x},{y - 8.0} {left_x + 2.0},{y - 3.0} {left_x + 7.0},{y - 3.0} {left_x + 3.0},{y} {left_x + 4.0},{y + 5.0} {left_x},{y + 2.0} {left_x - 4.0},{y + 5.0} {left_x - 3.0},{y} {left_x - 7.0},{y - 3.0} {left_x - 2.0},{y - 3.0}",
                fill: "#fbbf24",
                stroke: "#f59e0b",
                stroke_width: "0.5",
            }
            polygon {
                points: "{right_x},{y - 8.0} {right_x + 2.0},{y - 3.0} {right_x + 7.0},{y - 3.0} {right_x + 3.0},{y} {right_x + 4.0},{y + 5.0} {right_x},{y + 2.0} {right_x - 4.0},{y + 5.0} {right_x - 3.0},{y} {right_x - 7.0},{y - 3.0} {right_x - 2.0},{y - 3.0}",
                fill: "#fbbf24",
                stroke: "#f59e0b",
                stroke_width: "0.5",
            }
            circle { cx: "{left_x - 1.6}", cy: "{y - 2.4}", r: "1.2", fill: "white", opacity: "0.7" }
            circle { cx: "{right_x - 1.6}", cy: "{y - 2.4}", r: "1.2", fill: "white", opacity: "0.7" }
        },
        EyeType::Surprised => rsx! {
            circle { cx: "{left_x}", cy: "{y}", r: "9", fill: "white", stroke: "{eye_color}", stroke_width: "2" }
            circle { cx: "{left_x}", cy: "{y}", r: "4.5", fill: "{eye_color}" }
            circle { cx: "{left_x - 2.0}", cy: "{y - 2.0}", r: "1.5", fill: "white" }
            circle { cx: "{right_x}", cy: "{y}", r: "9", fill: "white", stroke: "{eye_color}", stroke_width: "2" }
            circle { cx: "{right_x}", cy: "{y}", r: "4.5", fill: "{eye_color}" }
            circle { cx: "{right_x - 2.0}", cy: "{y - 2.0}", r: "1.5", fill: "white" }
        },
        EyeType::Curious => rsx! {
            circle { cx: "{left_x}", cy: "{y}", r: "7", fill: "{eye_color}" }
            circle { cx: "{left_x + 2.0}", cy: "{y - 2.0}", r: "2.5", fill: "white" }
            circle { cx: "{right_x}", cy: "{y}", r: "7", fill: "{eye_color}" }
            circle { cx: "{right_x - 2.0}", cy: "{y - 2.0}", r: "2.5", fill: "white" }
        },
        EyeType::Bored => rsx! {
            ellipse { cx: "{left_x}", cy: "{y}", rx: "7", ry: "4", fill: "{eye_color}" }
            line { x1: "{left_x - 8.0}", y1: "{y - 3.0}", x2: "{left_x + 8.0}", y2: "{y - 3.0}", stroke: "{eye_color}", stroke_width: "2" }
            ellipse { cx: "{right_x}", cy: "{y}", rx: "7", ry: "4", fill: "{eye_color}" }
            line { x1: "{right_x - 8.0}", y1: "{y - 3.0}", x2: "{right_x + 8.0}", y2: "{y - 3.0}", stroke: "{eye_color}", stroke_width: "2" }
        },
        _ => rsx! {
            circle { cx: "{left_x}", cy: "{y}", r: "7", fill: "{eye_color}" }
            circle { cx: "{left_x - 2.0}", cy: "{y - 2.0}", r: "2", fill: "white", opacity: "0.6" }
            circle { cx: "{right_x}", cy: "{y}", r: "7", fill: "{eye_color}" }
            circle { cx: "{right_x - 2.0}", cy: "{y - 2.0}", r: "2", fill: "white", opacity: "0.6" }
        },
    }
}

fn render_adult_mouth(mouth_type: MouthType, cx: f64, cy: f64) -> Element {
    match mouth_type {
        MouthType::Smile => rsx! {
            path { d: "M {cx - 10.0} {cy} Q {cx} {cy + 10.0} {cx + 10.0} {cy}", fill: "none", stroke: "currentColor", stroke_width: "2.5", stroke_linecap: "round", class: "text-foreground/60" }
        },
        MouthType::Grin => rsx! {
            path { d: "M {cx - 14.0} {cy - 2.0} Q {cx} {cy + 14.0} {cx + 14.0} {cy - 2.0}", fill: "none", stroke: "currentColor", stroke_width: "2.5", stroke_linecap: "round", class: "text-foreground/60" }
        },
        MouthType::Frown => rsx! {
            path { d: "M {cx - 10.0} {cy + 5.0} Q {cx} {cy - 3.0} {cx + 10.0} {cy + 5.0}", fill: "none", stroke: "currentColor", stroke_width: "2.5", stroke_linecap: "round", class: "text-foreground/60" }
        },
        MouthType::Open => rsx! {
            ellipse { cx: "{cx}", cy: "{cy}", rx: "8", ry: "10", fill: "currentColor", class: "text-foreground/40" }
        },
        MouthType::Sad => rsx! {
            path { d: "M {cx - 10.0} {cy + 5.0} Q {cx} {cy - 3.0} {cx + 10.0} {cy + 5.0}", fill: "none", stroke: "currentColor", stroke_width: "2.5", stroke_linecap: "round", class: "text-foreground/60" }
        },
        MouthType::Droopy => rsx! {
            path { d: "M {cx - 10.0} {cy + 2.0} Q {cx - 5.0} {cy - 1.0} {cx} {cy + 1.0} Q {cx + 5.0} {cy - 1.0} {cx + 10.0} {cy + 2.0}", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", class: "text-foreground/60" }
        },
        MouthType::Sleepy => rsx! {
            path { d: "M {cx - 8.0} {cy} Q {cx} {cy - 3.0} {cx + 8.0} {cy}", fill: "none", stroke: "currentColor", stroke_width: "1.5", stroke_linecap: "round", class: "text-foreground/40" }
        },
        MouthType::Round => rsx! {
            circle { cx: "{cx}", cy: "{cy}", r: "5", fill: "currentColor", class: "text-foreground/40" }
        },
        MouthType::Small => rsx! {
            path { d: "M {cx - 5.0} {cy} Q {cx} {cy + 4.0} {cx + 5.0} {cy}", fill: "none", stroke: "currentColor", stroke_width: "1.5", stroke_linecap: "round", class: "text-foreground/60" }
        },
        MouthType::Smirk => rsx! {
            path { d: "M {cx - 8.0} {cy} Q {cx} {cy + 6.0} {cx + 8.0} {cy - 2.0} Q {cx + 4.0} {cy + 1.0} {cx + 2.0} {cy}", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", class: "text-foreground/60" }
        },
        _ => rsx! {
            line { x1: "{cx - 8.0}", y1: "{cy}", x2: "{cx + 8.0}", y2: "{cy}", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", class: "text-foreground/60" }
        },
    }
}

use super::utils::lighten;

fn render_adult_eyebrow(config: &Option<EyebrowConfig>, left_x: f64, right_x: f64, y: f64) -> Element {
    match config {
        Some(brow) => {
            let angle = brow.angle;
            let oy = brow.offset_y;
            let by = y + oy;
            let opacity = if brow.worried { "0.7" } else { "0.5" };
            let curve = if brow.worried { 2.0 } else { 0.0 };
            let half_len = 12.0;
            rsx! {
                g {
                    path {
                        d: "M {left_x - half_len} {by + angle * 0.3} Q {left_x} {by + angle * 0.3 - curve - 3.0} {left_x + half_len} {by - angle * 0.3}",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2.5",
                        stroke_linecap: "round",
                        class: "text-foreground",
                        opacity: "{opacity}",
                    }
                    path {
                        d: "M {right_x - half_len} {by - angle * 0.3} Q {right_x} {by - angle * 0.3 - curve - 3.0} {right_x + half_len} {by + angle * 0.3}",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2.5",
                        stroke_linecap: "round",
                        class: "text-foreground",
                        opacity: "{opacity}",
                    }
                }
            }
        }
        None => rsx! { g {} },
    }
}

fn render_adult_extras(extras: &Extras, left_x: f64, right_x: f64, eye_y: f64, mouth_y: f64) -> Element {
    let mut parts: Vec<Element> = Vec::new();

    if let Some(tears) = &extras.tears {
        let (left_tears, right_tears) = match tears.eye {
            TearEye::Both => (true, true),
            TearEye::Left => (true, false),
            TearEye::Right => (false, true),
            TearEye::Alternating => (true, true),
        };
        let alt_class = matches!(tears.eye, TearEye::Alternating)
            .then(|| " animate-[blobbi-tear-alt_1.5s_ease-in-out_infinite]")
            .unwrap_or("");
        if left_tears {
            parts.push(rsx! {
                g { class: "animate-[blobbi-tear-fall_2s_ease-in_infinite]{alt_class}",
                    ellipse { cx: "{left_x}", cy: "{eye_y + 12.0}", rx: "2", ry: "3", fill: "rgba(100,180,255,0.6)" }
                    ellipse { cx: "{left_x - 1.0}", cy: "{eye_y + 20.0}", rx: "1.5", ry: "2", fill: "rgba(100,180,255,0.4)" }
                }
            });
        }
        if right_tears {
            let delay = if matches!(tears.eye, TearEye::Alternating) {
                " animation-delay: 0.75s;"
            } else {
                ""
            };
            parts.push(rsx! {
                g { class: "animate-[blobbi-tear-fall_2s_ease-in_infinite]",
                    style: "{delay}",
                    ellipse { cx: "{right_x}", cy: "{eye_y + 12.0}", rx: "2", ry: "3", fill: "rgba(100,180,255,0.6)" }
                    ellipse { cx: "{right_x + 1.0}", cy: "{eye_y + 20.0}", rx: "1.5", ry: "2", fill: "rgba(100,180,255,0.4)" }
                }
            });
        }
    }

    if extras.drool {
        parts.push(rsx! {
            g { class: "animate-[blobbi-drool_3s_ease-in-out_infinite]",
                ellipse { cx: "{(left_x + right_x) / 2.0 + 2.0}", cy: "{mouth_y + 8.0}", rx: "2", ry: "4", fill: "rgba(100,200,255,0.5)" }
            }
        });
    }

    if extras.food_icon {
        let mx = (left_x + right_x) / 2.0 + 18.0;
        parts.push(rsx! {
            text {
                x: "{mx}",
                y: "{mouth_y + 4.0}",
                font_size: "10",
                "🍽️",
            }
        });
    }

    if parts.is_empty() {
        rsx! { g {} }
    } else {
        rsx! { g { {parts.into_iter()} } }
    }
}

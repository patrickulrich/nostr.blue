use dioxus::prelude::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Poop {
    pub id: String,
    pub room: String,
    pub position: PoopPosition,
    pub created_at: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoopPosition {
    pub bottom: f64,
    pub left: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoopState {
    pub poops: Vec<Poop>,
    pub shovel_mode: bool,
}

impl PoopState {
    const SAFE_POSITIONS: &'static [(f64, f64)] = &[
        (8.0, 8.0),
        (12.0, 75.0),
        (5.0, 40.0),
        (15.0, 20.0),
        (10.0, 60.0),
    ];

    pub fn maybe_generate(&mut self, hunger: f64, last_meal: Option<u64>, now_secs: u64, current_room: &str) {
        if hunger >= 95.0 {
            let id = format!("poop-{}", now_secs);
            let pos_idx = self.poops.len() % Self::SAFE_POSITIONS.len();
            let (bottom, left) = Self::SAFE_POSITIONS[pos_idx];
            self.poops.push(Poop {
                id,
                room: current_room.to_string(),
                position: PoopPosition { bottom, left },
                created_at: now_secs,
            });
        }

        if let Some(last) = last_meal {
            let elapsed = now_secs.saturating_sub(last);
            if elapsed >= 7200 {
                let room = match self.poops.len() % 5 {
                    0 => "kitchen",
                    1 => "care",
                    2 => "home",
                    3 => "hatchery",
                    _ => "rest",
                };
                let id = format!("poop-time-{}", now_secs);
                let pos_idx = self.poops.len() % Self::SAFE_POSITIONS.len();
                let (bottom, left) = Self::SAFE_POSITIONS[pos_idx];
                self.poops.push(Poop {
                    id,
                    room: room.to_string(),
                    position: PoopPosition { bottom, left },
                    created_at: now_secs,
                });
            }
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.poops.retain(|p| p.id != id);
    }

    pub fn poops_in_room(&self, room: &str) -> Vec<&Poop> {
        self.poops.iter().filter(|p| p.room == room).collect()
    }

    pub fn has_any(&self) -> bool {
        !self.poops.is_empty()
    }
}

#[component]
pub fn PoopButton(poop: Poop, shovel_mode: bool, on_remove: EventHandler<String>) -> Element {
    rsx! {
        button {
            class: if shovel_mode {
                "absolute z-10 transition-all duration-300 cursor-pointer hover:scale-150 active:scale-75"
            } else {
                "absolute z-10 transition-all duration-300 pointer-events-none"
            },
            style: "bottom: {poop.position.bottom}%; left: {poop.position.left}%",
            onclick: {
                let id = poop.id.clone();
                move |_| {
                    if shovel_mode {
                        on_remove.call(id.clone());
                    }
                }
            },
            span { class: if shovel_mode { "text-2xl sm:text-3xl block drop-shadow-lg" } else { "text-2xl sm:text-3xl block" },
                "\u{1F4A9}"
            }
        }
    }
}

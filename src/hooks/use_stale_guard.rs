use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub struct StaleGuard {
    generation: Signal<u64>,
}

pub fn use_stale_guard() -> StaleGuard {
    let generation = use_signal(|| 0u64);
    StaleGuard { generation }
}

impl StaleGuard {
    pub fn bump(&mut self) -> u64 {
        let next = self.generation.peek().wrapping_add(1);
        self.generation.set(next);
        next
    }

    pub fn is_stale(&self, token: u64) -> bool {
        *self.generation.peek() != token
    }
}

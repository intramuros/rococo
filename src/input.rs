use macroquad::prelude::*;

/// One frame of player intent. Held keys are levels; the rest are presses.
#[derive(Clone, Copy, Default, Debug)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub down: bool,
    pub jump_held: bool,
    pub jump: bool,
    pub attack: bool,
    pub cast: bool,
    pub heal: bool,
    pub interact: bool,
    pub confirm: bool,
}

impl Input {
    pub fn poll() -> Self {
        let down = |keys: &[KeyCode]| keys.iter().any(|k| is_key_down(*k));
        let pressed = |keys: &[KeyCode]| keys.iter().any(|k| is_key_pressed(*k));
        Self {
            left: down(&[KeyCode::A, KeyCode::Left]),
            right: down(&[KeyCode::D, KeyCode::Right]),
            down: down(&[KeyCode::S, KeyCode::Down]),
            jump_held: down(&[KeyCode::Space, KeyCode::Z]),
            jump: pressed(&[KeyCode::Space, KeyCode::Z]),
            attack: pressed(&[KeyCode::J, KeyCode::X]),
            cast: pressed(&[KeyCode::K, KeyCode::C]),
            heal: pressed(&[KeyCode::L, KeyCode::V]),
            interact: pressed(&[KeyCode::E, KeyCode::W, KeyCode::Up]),
            confirm: pressed(&[KeyCode::Enter, KeyCode::KpEnter, KeyCode::Space]),
        }
    }

    /// Accumulates presses from `newer` so none are lost on frames where the
    /// fixed-timestep loop runs no update; held state comes from `newer`.
    pub fn merge(self, newer: Self) -> Self {
        Self {
            jump: self.jump || newer.jump,
            attack: self.attack || newer.attack,
            cast: self.cast || newer.cast,
            heal: self.heal || newer.heal,
            interact: self.interact || newer.interact,
            confirm: self.confirm || newer.confirm,
            ..newer
        }
    }

    /// The same held state with presses cleared, for the extra substeps of a frame.
    pub fn held_only(self) -> Self {
        Self { jump: false, attack: false, cast: false, heal: false, interact: false, confirm: false, ..self }
    }
}

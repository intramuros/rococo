//! Éloise: run, jump, rapier lunge, petal darts and rosewater.

use macroquad::prelude::*;

use crate::input::Input;
use crate::world::{self, Room, approach};

pub const PW: f32 = 10.0;
pub const PH: f32 = 22.0;
pub const MAX_HP: f32 = 100.0;
pub const MAX_MANA: f32 = 100.0;
pub const PETAL_COST: f32 = 25.0;
pub const MAX_FLASKS: u32 = 3;
pub const FLASK_HEAL: f32 = 40.0;
pub const ATTACK_TIME: f32 = 0.24;
pub const GRAVITY: f32 = 1100.0;
pub const MAX_FALL: f32 = 330.0;

const RUN_SPEED: f32 = 115.0;
const ACCEL_GROUND: f32 = 1500.0;
const ACCEL_AIR: f32 = 1000.0;
/// Reaches ~59px (3.7 tiles): enough for 3-tile steps, never 6.
const JUMP_SPEED: f32 = 360.0;
/// The silk ribbon's second jump, ~55px more.
const AIR_JUMP_SPEED: f32 = 345.0;
/// Upward speed is capped to this when jump is released early.
const JUMP_CUT_SPEED: f32 = 120.0;
const COYOTE_TIME: f32 = 0.09;
const JUMP_BUFFER: f32 = 0.12;
const ATTACK_COOLDOWN: f32 = 0.30;
const INVULN_TIME: f32 = 1.0;
const HURT_STUN: f32 = 0.25;

#[derive(Default, Debug)]
pub struct PlayerEvents {
    pub jumped: bool,
    pub double_jumped: bool,
    pub landed: bool,
    pub swung: bool,
    pub cast: bool,
    pub healed: bool,
}

pub struct Player {
    pub pos: Vec2,
    pub vel: Vec2,
    pub facing: f32,
    pub on_ground: bool,
    coyote: f32,
    jump_buffer: f32,
    pub air_jumps: u32,
    drop_t: f32,
    pub hp: f32,
    pub mana: f32,
    pub flasks: u32,
    pub invuln: f32,
    stun: f32,
    /// Seconds since the current lunge began.
    pub attack_t: Option<f32>,
    attack_cd: f32,
    pub cast_t: f32,
    pub heal_t: f32,
    pub anim: f32,
}

impl Player {
    pub fn new(feet: Vec2) -> Self {
        Self {
            pos: feet - vec2(PW / 2.0, PH),
            vel: Vec2::ZERO,
            facing: 1.0,
            on_ground: false,
            coyote: 0.0,
            jump_buffer: 0.0,
            air_jumps: 0,
            drop_t: 0.0,
            hp: MAX_HP,
            mana: MAX_MANA * 0.5,
            flasks: MAX_FLASKS,
            invuln: 0.0,
            stun: 0.0,
            attack_t: None,
            attack_cd: 0.0,
            cast_t: 0.0,
            heal_t: 0.0,
            anim: 0.0,
        }
    }

    pub fn place(&mut self, feet: Vec2) {
        self.pos = feet - vec2(PW / 2.0, PH);
        self.vel = Vec2::ZERO;
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.pos.x, self.pos.y, PW, PH)
    }

    pub fn center(&self) -> Vec2 {
        self.pos + vec2(PW, PH) / 2.0
    }

    pub fn feet(&self) -> Vec2 {
        self.pos + vec2(PW / 2.0, PH)
    }

    /// The rapier's reach during the active part of a lunge.
    pub fn attack_box(&self) -> Option<Rect> {
        let t = self.attack_t?;
        if !(0.03..=0.15).contains(&t) {
            return None;
        }
        let x = if self.facing > 0.0 { self.pos.x + PW - 2.0 } else { self.pos.x - 22.0 };
        Some(Rect::new(x, self.pos.y + 4.0, 24.0, 10.0))
    }

    /// Applies damage unless invulnerable. Returns whether it landed.
    pub fn hurt(&mut self, dmg: f32, from_x: f32) -> bool {
        if self.invuln > 0.0 || self.hp <= 0.0 {
            return false;
        }
        self.hp = (self.hp - dmg).max(0.0);
        self.invuln = INVULN_TIME;
        self.stun = HURT_STUN;
        self.attack_t = None;
        let away = if self.center().x >= from_x { 1.0 } else { -1.0 };
        self.vel = vec2(away * 140.0, -170.0);
        true
    }

    pub fn update(&mut self, inp: &Input, room: &Room, has_ribbon: bool, dt: f32) -> PlayerEvents {
        let mut ev = PlayerEvents::default();
        self.anim += dt;
        self.invuln = (self.invuln - dt).max(0.0);
        self.stun = (self.stun - dt).max(0.0);
        self.attack_cd -= dt;
        self.cast_t = (self.cast_t - dt).max(0.0);
        self.heal_t = (self.heal_t - dt).max(0.0);
        let control = self.stun <= 0.0;

        if control {
            let dir = inp.right as i32 as f32 - inp.left as i32 as f32;
            let accel = if self.on_ground { ACCEL_GROUND } else { ACCEL_AIR };
            self.vel.x = approach(self.vel.x, dir * RUN_SPEED, accel * dt);
            if dir != 0.0 && self.attack_t.is_none() {
                self.facing = dir;
            }
        }

        if inp.jump {
            self.jump_buffer = JUMP_BUFFER;
        } else {
            self.jump_buffer -= dt;
        }
        if self.on_ground {
            self.coyote = COYOTE_TIME;
            self.air_jumps = has_ribbon as u32;
        } else {
            self.coyote -= dt;
        }
        if control && self.jump_buffer > 0.0 {
            let size = vec2(PW, PH);
            if inp.down && self.on_ground && world::standing_on_one_way(room, self.pos, size) {
                self.drop_t = 0.15;
                self.jump_buffer = 0.0;
            } else if self.coyote > 0.0 {
                self.vel.y = -JUMP_SPEED;
                self.coyote = 0.0;
                self.jump_buffer = 0.0;
                ev.jumped = true;
            } else if self.air_jumps > 0 {
                self.vel.y = -AIR_JUMP_SPEED;
                self.air_jumps -= 1;
                self.jump_buffer = 0.0;
                ev.double_jumped = true;
            }
        }
        if !inp.jump_held && self.vel.y < -JUMP_CUT_SPEED && control {
            self.vel.y = -JUMP_CUT_SPEED;
        }
        self.vel.y = (self.vel.y + GRAVITY * dt).min(MAX_FALL);

        if control && inp.attack && self.attack_cd <= 0.0 {
            self.attack_t = Some(0.0);
            self.attack_cd = ATTACK_COOLDOWN;
            if self.on_ground {
                self.vel.x += self.facing * 70.0;
            }
            ev.swung = true;
        }
        if let Some(t) = &mut self.attack_t {
            *t += dt;
            if *t > ATTACK_TIME {
                self.attack_t = None;
            }
        }
        if control && inp.cast && self.mana >= PETAL_COST {
            self.mana -= PETAL_COST;
            self.cast_t = 0.25;
            ev.cast = true;
        }
        if control && inp.heal && self.flasks > 0 && self.hp < MAX_HP {
            self.flasks -= 1;
            self.hp = (self.hp + FLASK_HEAL).min(MAX_HP);
            self.heal_t = 0.6;
            ev.healed = true;
        }

        let was_on_ground = self.on_ground;
        let contacts = world::move_body(room, &mut self.pos, vec2(PW, PH), &mut self.vel, dt, self.drop_t > 0.0);
        self.drop_t -= dt;
        self.on_ground = contacts.ground;
        ev.landed = contacts.ground && !was_on_ground;
        ev
    }
}

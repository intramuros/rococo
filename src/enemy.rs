//! The palace's residents: halberdier guards, library moths, and the
//! Masked Marquise who hosts the endless masque.

use macroquad::prelude::*;

use crate::game::{ProjKind, Projectile, Rng};
use crate::player::{GRAVITY, MAX_FALL};
use crate::world::{self, Room, TILE, Tile, approach};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyKind {
    Halberdier,
    Moth,
    Marquise,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ai {
    // Halberdier
    Patrol,
    Windup,
    Strike,
    Recover,
    // Moth
    Hover,
    // Marquise (also uses Recover)
    Dormant,
    Idle,
    FanWindup,
    FanOut,
    LungeWindup,
    Lunge,
    VolleyWindup,
    Dying,
}

pub const HALBERD_WINDUP: f32 = 0.55;
pub const MARQUISE_DEATH_TIME: f32 = 2.4;

pub struct Enemy {
    pub kind: EnemyKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub size: Vec2,
    pub facing: f32,
    pub hp: i32,
    pub max_hp: i32,
    pub ai: Ai,
    /// Seconds spent in the current `ai` state.
    pub t: f32,
    pub anim: f32,
    pub flash: f32,
    stagger: f32,
    /// The player's swing id that last hit us, so one lunge hits once.
    pub last_swing: u32,
    home: Vec2,
    last_attack: Ai,
    pub dead: bool,
}

impl Enemy {
    pub fn new(kind: EnemyKind, feet: Vec2) -> Self {
        let (size, hp, ai) = match kind {
            EnemyKind::Halberdier => (vec2(14.0, 30.0), 6, Ai::Patrol),
            EnemyKind::Moth => (vec2(10.0, 8.0), 2, Ai::Hover),
            EnemyKind::Marquise => (vec2(26.0, 50.0), 44, Ai::Dormant),
        };
        let pos = match kind {
            EnemyKind::Moth => feet - vec2(size.x / 2.0, TILE / 2.0 + size.y / 2.0),
            _ => feet - vec2(size.x / 2.0, size.y),
        };
        Self {
            kind,
            pos,
            vel: Vec2::ZERO,
            size,
            facing: -1.0,
            hp,
            max_hp: hp,
            ai,
            t: 0.0,
            anim: feet.x * 0.37,
            flash: 0.0,
            stagger: 0.0,
            last_swing: 0,
            home: pos,
            last_attack: Ai::Idle,
            dead: false,
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.pos.x, self.pos.y, self.size.x, self.size.y)
    }

    pub fn center(&self) -> Vec2 {
        self.pos + self.size / 2.0
    }

    pub fn feet(&self) -> Vec2 {
        self.pos + vec2(self.size.x / 2.0, self.size.y)
    }

    pub fn alive(&self) -> bool {
        !self.dead && self.ai != Ai::Dying
    }

    pub fn phase_two(&self) -> bool {
        self.kind == EnemyKind::Marquise && self.hp * 2 <= self.max_hp
    }

    /// Whether touching this enemy hurts right now.
    pub fn harmful(&self) -> bool {
        self.alive() && self.stagger <= 0.0 && self.ai != Ai::Dormant
    }

    pub fn contact_damage(&self) -> f32 {
        match (self.kind, self.ai) {
            (EnemyKind::Marquise, Ai::Lunge) => 20.0,
            (EnemyKind::Marquise, _) => 14.0,
            (EnemyKind::Halberdier, _) => 12.0,
            (EnemyKind::Moth, _) => 10.0,
        }
    }

    /// A weapon hitbox beyond the body, and its damage.
    pub fn attack_box(&self) -> Option<(Rect, f32)> {
        match (self.kind, self.ai) {
            (EnemyKind::Halberdier, Ai::Strike) if (0.03..=0.2).contains(&self.t) => {
                let x = if self.facing > 0.0 { self.pos.x + self.size.x } else { self.pos.x - 40.0 };
                Some((Rect::new(x, self.pos.y + 10.0, 40.0, 6.0), 20.0))
            }
            (EnemyKind::Marquise, Ai::Lunge) => {
                let x = if self.facing > 0.0 { self.pos.x + self.size.x } else { self.pos.x - 22.0 };
                Some((Rect::new(x, self.pos.y + 18.0, 22.0, 6.0), 20.0))
            }
            _ => None,
        }
    }

    pub fn wake(&mut self) {
        if self.ai == Ai::Dormant {
            self.set(Ai::Idle);
        }
    }

    fn set(&mut self, ai: Ai) {
        self.ai = ai;
        self.t = 0.0;
    }

    pub fn hit(&mut self, dmg: i32, dir: f32) {
        if !self.alive() {
            return;
        }
        self.hp -= dmg;
        self.flash = 0.12;
        // Guards keep their footing mid-swing; that's the opening to punish.
        let armoured = matches!(self.ai, Ai::Windup | Ai::Strike);
        if self.kind != EnemyKind::Marquise && !armoured {
            self.stagger = 0.22;
            self.vel.x = dir * 130.0;
            if self.kind == EnemyKind::Moth {
                self.vel.y = -50.0;
            }
        }
        if self.hp <= 0 {
            match self.kind {
                EnemyKind::Marquise => self.set(Ai::Dying),
                _ => self.dead = true,
            }
        }
    }

    /// Advances AI and physics. Returns how much to shake the screen.
    pub fn update(&mut self, dt: f32, player: Rect, room: &Room, rng: &mut Rng, shots: &mut Vec<Projectile>) -> f32 {
        self.anim += dt;
        self.flash = (self.flash - dt).max(0.0);
        let target = player.center();
        match self.kind {
            EnemyKind::Halberdier => {
                self.update_halberdier(dt, target, room);
                0.0
            }
            EnemyKind::Moth => {
                self.update_moth(dt, target, room);
                0.0
            }
            EnemyKind::Marquise => self.update_marquise(dt, target, room, rng, shots),
        }
    }

    fn face(&mut self, x: f32) {
        let dx = x - self.center().x;
        if dx.abs() > 1.0 {
            self.facing = dx.signum();
        }
    }

    fn fall(&mut self, dt: f32, room: &Room) -> world::Contacts {
        self.vel.y = (self.vel.y + GRAVITY * dt).min(MAX_FALL);
        world::move_body(room, &mut self.pos, self.size, &mut self.vel, dt, false)
    }

    fn update_halberdier(&mut self, dt: f32, target: Vec2, room: &Room) {
        if self.stagger > 0.0 {
            self.stagger -= dt;
            self.vel.x = approach(self.vel.x, 0.0, 600.0 * dt);
        } else {
            self.t += dt;
            match self.ai {
                Ai::Patrol => {
                    self.vel.x = self.facing * 28.0;
                    let ahead = if self.facing > 0.0 { self.pos.x + self.size.x + 1.0 } else { self.pos.x - 1.0 };
                    let col = (ahead / TILE).floor() as i32;
                    let body_row = ((self.pos.y + self.size.y - 1.0) / TILE).floor() as i32;
                    let floor_row = ((self.pos.y + self.size.y + 1.0) / TILE).floor() as i32;
                    let blocked = room.tile(col, body_row) == Tile::Solid || room.tile(col, floor_row) == Tile::Empty;
                    let strayed = (self.pos.x - self.home.x) * self.facing > 80.0;
                    if blocked || strayed {
                        self.facing = -self.facing;
                    }
                    let d = target - self.center();
                    if d.x.abs() < 80.0 && d.y.abs() < 40.0 {
                        self.face(target.x);
                        self.set(Ai::Windup);
                    }
                }
                Ai::Windup => {
                    self.vel.x = 0.0;
                    if self.t > HALBERD_WINDUP {
                        self.set(Ai::Strike);
                    }
                }
                Ai::Strike => {
                    self.vel.x = if self.t < 0.1 { self.facing * 50.0 } else { 0.0 };
                    if self.t > 0.3 {
                        self.set(Ai::Recover);
                    }
                }
                Ai::Recover => {
                    self.vel.x = 0.0;
                    if self.t > 0.65 {
                        self.set(Ai::Patrol);
                    }
                }
                _ => {}
            }
        }
        self.fall(dt, room);
    }

    fn update_moth(&mut self, dt: f32, target: Vec2, room: &Room) {
        self.t += dt;
        if self.stagger > 0.0 {
            self.stagger -= dt;
            self.vel *= (1.0 - 4.0 * dt).max(0.0);
        } else {
            let me = self.center();
            let goal = if target.distance(me) < 140.0 {
                target
            } else {
                self.home + self.size / 2.0 + vec2((self.anim * 0.8).sin() * 24.0, (self.anim * 1.3).sin() * 10.0)
            };
            let desired = (goal - me).normalize_or_zero() * 46.0 + vec2(0.0, (self.anim * 5.0).sin() * 25.0);
            self.vel = self.vel.lerp(desired, (3.0 * dt).min(1.0));
            self.face(target.x);
        }
        world::move_body(room, &mut self.pos, self.size, &mut self.vel, dt, true);
    }

    fn update_marquise(
        &mut self,
        dt: f32,
        target: Vec2,
        room: &Room,
        rng: &mut Rng,
        shots: &mut Vec<Projectile>,
    ) -> f32 {
        self.t += dt;
        let pace = if self.phase_two() { 1.35 } else { 1.0 };
        let mut shake = 0.0;
        match self.ai {
            Ai::Dormant => {
                self.vel.x = 0.0;
                self.face(target.x);
            }
            Ai::Idle => {
                self.face(target.x);
                let dx = target.x - self.center().x;
                let want = if dx.abs() < 90.0 {
                    -dx.signum()
                } else if dx.abs() > 150.0 {
                    dx.signum()
                } else {
                    0.0
                };
                self.vel.x = approach(self.vel.x, want * 60.0, 400.0 * dt);
                if self.t > 0.9 / pace {
                    self.vel.x = 0.0;
                    let next = self.pick_attack(rng);
                    self.set(next);
                }
            }
            Ai::FanWindup => {
                self.vel.x = approach(self.vel.x, 0.0, 600.0 * dt);
                if self.t > 0.45 / pace {
                    let feet = self.feet();
                    shots.push(Projectile::new(
                        ProjKind::Fan,
                        vec2(feet.x + self.facing * 16.0, feet.y - 13.0),
                        vec2(self.facing * 280.0, 0.0),
                        false,
                    ));
                    self.set(Ai::FanOut);
                }
            }
            Ai::FanOut => {
                self.vel.x = 0.0;
                if self.t > 1.1 / pace {
                    self.set(Ai::Idle);
                }
            }
            Ai::LungeWindup => {
                self.vel.x = 0.0;
                if self.t < 0.2 {
                    self.face(target.x);
                }
                if self.t > 0.65 / pace {
                    self.set(Ai::Lunge);
                    shake = 1.5;
                }
            }
            Ai::Lunge => {
                self.vel.x = self.facing * 360.0;
                if self.t > 0.55 {
                    self.set(Ai::Recover);
                }
            }
            Ai::VolleyWindup => {
                self.vel.x = 0.0;
                self.face(target.x);
                if self.t > 0.5 / pace {
                    let from = self.pos + vec2(self.size.x / 2.0, 12.0);
                    let aim = (target - from).normalize_or(vec2(self.facing, 0.0));
                    let base = aim.y.atan2(aim.x);
                    for i in -2..=2 {
                        let a = base + i as f32 * 0.18;
                        shots.push(Projectile::new(ProjKind::Thorn, from, vec2(a.cos(), a.sin()) * 165.0, false));
                    }
                    self.set(Ai::Recover);
                }
            }
            Ai::Recover => {
                self.vel.x = approach(self.vel.x, 0.0, 900.0 * dt);
                if self.t > 0.6 / pace {
                    self.set(Ai::Idle);
                }
            }
            Ai::Dying => self.vel.x = 0.0,
            _ => {}
        }
        let contacts = self.fall(dt, room);
        if self.ai == Ai::Lunge && contacts.wall {
            self.set(Ai::Recover);
            shake = 3.0;
        }
        shake
    }

    fn pick_attack(&mut self, rng: &mut Rng) -> Ai {
        let mut options = vec![Ai::FanWindup, Ai::LungeWindup];
        if self.phase_two() {
            options.push(Ai::VolleyWindup);
        }
        // Never the same attack twice in a row, so patterns stay readable.
        options.retain(|a| *a != self.last_attack);
        let next = rng.pick(&options);
        self.last_attack = next;
        next
    }
}

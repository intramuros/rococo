//! Game state and rules. Nothing here touches the window or GPU, so it runs
//! headless in tests.

use macroquad::prelude::*;

use crate::enemy::{Ai, Enemy, EnemyKind, MARQUISE_DEATH_TIME};
use crate::input::Input;
use crate::palette::GOLD;
use crate::palette::*;
use crate::player::{self, Player};
use crate::world::{self, Room, RoomId, Side, SpawnKind, TILE, Theme, Tile, approach};
use crate::{VIEW_H, VIEW_W};

/// Seconds for each half of the fade between rooms.
pub const FADE: f32 = 0.28;
const BANNER_TIME: f32 = 3.2;
const BOSS_INTRO_TIME: f32 = 2.4;

/// A small xorshift generator so simulations are reproducible.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.next_u32() as usize % items.len()]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Popup {
    Relic,
    Letter,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Scene {
    Title,
    Playing,
    Popup(Popup),
    Dead(f32),
    Victory(f32),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fight {
    None,
    Intro(f32),
    Active,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InterKind {
    Relic,
    Letter,
}

pub struct Interactable {
    pub kind: InterKind,
    pub pos: Vec2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjKind {
    /// Éloise's rose-petal dart.
    Petal,
    /// The Marquise's boomerang fan.
    Fan,
    /// The Marquise's crimson thorn volley.
    Thorn,
}

pub struct Projectile {
    pub kind: ProjKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub t: f32,
    pub friendly: bool,
    pub dmg: f32,
    pub radius: f32,
    dir: f32,
    pub dead: bool,
}

impl Projectile {
    pub fn new(kind: ProjKind, pos: Vec2, vel: Vec2, friendly: bool) -> Self {
        let (dmg, radius) = match kind {
            ProjKind::Petal => (2.0, 4.0),
            ProjKind::Fan => (15.0, 7.0),
            ProjKind::Thorn => (12.0, 3.0),
        };
        Self { kind, pos, vel, t: 0.0, friendly, dmg, radius, dir: vel.x.signum(), dead: false }
    }

    fn rect(&self) -> Rect {
        Rect::new(self.pos.x - self.radius, self.pos.y - self.radius, self.radius * 2.0, self.radius * 2.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PKind {
    Spark,
    Petal,
    Mote,
    Puff,
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub kind: PKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    pub color: Color,
    pub gravity: f32,
    pub seed: f32,
}

struct Transition {
    t: f32,
    to: RoomId,
    arrive: Vec2,
    swapped: bool,
}

pub struct Flags {
    pub ribbon: bool,
    pub letter_read: bool,
    pub boss_defeated: bool,
    visited: Vec<RoomId>,
}

pub struct Game {
    pub scene: Scene,
    pub room: Room,
    pub player: Player,
    pub enemies: Vec<Enemy>,
    pub projectiles: Vec<Projectile>,
    pub particles: Vec<Particle>,
    pub interactables: Vec<Interactable>,
    pub flags: Flags,
    pub fight: Fight,
    pub cam: Vec2,
    pub shake: f32,
    pub time: f32,
    pub play_time: f32,
    pub deaths: u32,
    pub hp_ghost: f32,
    /// Index into `interactables` of the one the player can examine.
    pub prompt: Option<usize>,
    pub banner: Option<(&'static str, f32)>,
    pub rng: Rng,
    transition: Option<Transition>,
    entry: (RoomId, Vec2),
    hitstop: f32,
    swing: u32,
    ambient: f32,
}

impl Game {
    pub fn new() -> Self {
        let room = world::build(RoomId::Gallery);
        let start = room.spawn(SpawnKind::Player).expect("gallery has a player start");
        Self {
            scene: Scene::Title,
            room,
            player: Player::new(start),
            enemies: Vec::new(),
            projectiles: Vec::new(),
            particles: Vec::new(),
            interactables: Vec::new(),
            flags: Flags { ribbon: false, letter_read: false, boss_defeated: false, visited: Vec::new() },
            fight: Fight::None,
            cam: Vec2::ZERO,
            shake: 0.0,
            time: 0.0,
            play_time: 0.0,
            deaths: 0,
            hp_ghost: player::MAX_HP,
            prompt: None,
            banner: None,
            rng: Rng::new(0x5EED_EC1A),
            transition: None,
            entry: (RoomId::Gallery, start),
            hitstop: 0.0,
            swing: 0,
            ambient: 0.0,
        }
    }

    pub fn start(&mut self) {
        let start = self.room.spawn(SpawnKind::Player).unwrap_or(self.entry.1);
        self.particles.clear();
        self.load_room(RoomId::Gallery, start);
        self.scene = Scene::Playing;
    }

    pub fn boss(&self) -> Option<&Enemy> {
        self.enemies.iter().find(|e| e.kind == EnemyKind::Marquise)
    }

    /// How dark the room-change fade is right now, 0..1.
    pub fn fade(&self) -> f32 {
        match &self.transition {
            Some(tr) if tr.t < FADE => tr.t / FADE,
            Some(tr) => (1.0 - (tr.t - FADE) / FADE).clamp(0.0, 1.0),
            None => 0.0,
        }
    }

    pub fn update(&mut self, dt: f32, inp: Input) {
        self.time += dt;
        match self.scene {
            Scene::Title => {
                self.update_particles(dt);
                self.spawn_ambient(dt);
                if inp.confirm {
                    self.start();
                }
            }
            Scene::Playing => self.update_play(dt, inp),
            Scene::Popup(_) => {
                if inp.confirm || inp.interact || inp.attack {
                    self.scene = Scene::Playing;
                }
            }
            Scene::Dead(t) => {
                self.update_particles(dt);
                self.scene = Scene::Dead(t + dt);
                if t > 1.2 && (inp.confirm || inp.attack) {
                    self.respawn();
                }
            }
            Scene::Victory(t) => {
                self.update_particles(dt);
                self.spawn_ambient(dt);
                self.scene = Scene::Victory(t + dt);
                if t > 3.0 && inp.confirm {
                    *self = Game::new();
                }
            }
        }
    }

    fn load_room(&mut self, id: RoomId, arrive: Vec2) {
        self.room = world::build(id);
        self.enemies.clear();
        self.projectiles.clear();
        self.interactables.clear();
        self.fight = Fight::None;
        for s in &self.room.spawns {
            let enemy = |kind| Some(Enemy::new(kind, s.pos));
            let item = |kind| Some(Interactable { kind, pos: s.pos });
            let (e, i) = match s.kind {
                SpawnKind::Player => (None, None),
                SpawnKind::Halberdier => (enemy(EnemyKind::Halberdier), None),
                SpawnKind::Moth => (enemy(EnemyKind::Moth), None),
                SpawnKind::Marquise if self.flags.boss_defeated => (None, None),
                SpawnKind::Marquise => (enemy(EnemyKind::Marquise), None),
                SpawnKind::Relic => (None, item(InterKind::Relic)),
                SpawnKind::Letter => (None, item(InterKind::Letter)),
            };
            self.enemies.extend(e);
            self.interactables.extend(i);
        }
        self.player.place(arrive);
        self.entry = (id, arrive);
        if !self.flags.visited.contains(&id) {
            self.flags.visited.push(id);
            self.banner = Some((self.room.name, 0.0));
        }
        self.cam = self.camera_target();
    }

    fn respawn(&mut self) {
        self.player.hp = player::MAX_HP;
        self.player.flasks = player::MAX_FLASKS;
        self.player.invuln = 0.0;
        self.hp_ghost = player::MAX_HP;
        self.particles.clear();
        let (id, at) = self.entry;
        self.load_room(id, at);
        self.scene = Scene::Playing;
    }

    fn update_play(&mut self, dt: f32, inp: Input) {
        self.play_time += dt;
        self.update_particles(dt);
        self.shake = (self.shake - dt * 14.0).max(0.0);
        if let Some((_, t)) = &mut self.banner {
            *t += dt;
            if *t > BANNER_TIME {
                self.banner = None;
            }
        }
        self.hp_ghost = if self.hp_ghost < self.player.hp {
            self.player.hp
        } else {
            approach(self.hp_ghost, self.player.hp, 40.0 * dt)
        };

        if let Some(mut tr) = self.transition.take() {
            tr.t += dt;
            if !tr.swapped && tr.t >= FADE {
                tr.swapped = true;
                self.load_room(tr.to, tr.arrive);
            }
            if tr.t < FADE * 2.0 {
                self.transition = Some(tr);
            }
            return;
        }
        if self.hitstop > 0.0 {
            self.hitstop -= dt;
            return;
        }

        let ev = self.player.update(&inp, &self.room, self.flags.ribbon, dt);
        self.player_effects(&ev);
        if self.interact(&inp) {
            return;
        }

        let target = self.player.rect();
        for e in &mut self.enemies {
            let shake = e.update(dt, target, &self.room, &mut self.rng, &mut self.projectiles);
            self.shake = self.shake.max(shake);
        }
        self.update_projectiles(dt);
        self.resolve_combat();
        self.update_fight(dt);
        self.check_exits();

        if self.player.hp <= 0.0 {
            self.deaths += 1;
            self.burst(self.player.center(), 24, PKind::Petal, ROSE, 120.0, 60.0, 1.6);
            self.scene = Scene::Dead(0.0);
        }
        let k = 1.0 - (-8.0 * dt).exp();
        self.cam += (self.camera_target() - self.cam) * k;
        self.spawn_ambient(dt);
    }

    fn player_effects(&mut self, ev: &player::PlayerEvents) {
        let feet = self.player.feet();
        if ev.jumped || ev.landed {
            self.burst(feet, 4, PKind::Puff, with_alpha(CREAM, 0.5), 30.0, -10.0, 0.35);
        }
        if ev.double_jumped {
            self.burst(feet, 10, PKind::Petal, ROSE, 70.0, 80.0, 0.8);
        }
        if ev.swung {
            self.swing += 1;
        }
        if ev.cast {
            let facing = self.player.facing;
            let from = self.player.center() + vec2(facing * 8.0, -3.0);
            self.projectiles.push(Projectile::new(ProjKind::Petal, from, vec2(facing * 300.0, 0.0), true));
            self.burst(from, 5, PKind::Petal, ROSE, 40.0, 0.0, 0.4);
        }
        if ev.healed {
            self.burst(self.player.center(), 14, PKind::Spark, GOLD_LIGHT, 40.0, -60.0, 0.9);
        }
    }

    /// Handles the examine prompt. Returns true if a popup opened.
    fn interact(&mut self, inp: &Input) -> bool {
        let feet = self.player.feet();
        self.prompt = self.interactables.iter().position(|it| {
            let taken = it.kind == InterKind::Relic && self.flags.ribbon;
            !taken && (it.pos.x - feet.x).abs() < 18.0 && (it.pos.y - feet.y).abs() < 24.0
        });
        let Some(i) = self.prompt else { return false };
        if !inp.interact || !self.player.on_ground {
            return false;
        }
        let pos = self.interactables[i].pos;
        match self.interactables[i].kind {
            InterKind::Relic => {
                self.flags.ribbon = true;
                self.player.air_jumps = 1;
                self.burst(pos - vec2(0.0, 20.0), 30, PKind::Petal, ROSE, 90.0, 40.0, 1.4);
                self.scene = Scene::Popup(Popup::Relic);
            }
            InterKind::Letter => {
                self.flags.letter_read = true;
                self.scene = Scene::Popup(Popup::Letter);
            }
        }
        self.prompt = None;
        true
    }

    fn update_projectiles(&mut self, dt: f32) {
        let room_w = self.room.px_w();
        for p in &mut self.projectiles {
            p.t += dt;
            match p.kind {
                ProjKind::Fan => {
                    // Flies out, slows, and sails back past its owner.
                    p.vel.x -= p.dir * 300.0 * dt;
                    p.pos += p.vel * dt;
                    p.pos.y += (p.t * 8.0).sin() * 0.25;
                    if p.t > 3.0 || p.pos.x < -20.0 || p.pos.x > room_w + 20.0 {
                        p.dead = true;
                    }
                }
                ProjKind::Petal | ProjKind::Thorn => {
                    p.pos += p.vel * dt;
                    if p.t > 2.5 || self.room.tile_at(p.pos) == Tile::Solid {
                        p.dead = true;
                    }
                }
            }
        }
    }

    fn resolve_combat(&mut self) {
        // Rapier against enemies.
        if let Some(blade) = self.player.attack_box() {
            let facing = self.player.facing;
            let mut hits = Vec::new();
            for e in &mut self.enemies {
                if e.alive() && e.last_swing != self.swing && e.rect().overlaps(&blade) {
                    e.last_swing = self.swing;
                    e.hit(1, facing);
                    hits.push(e.center().lerp(blade.center(), 0.5));
                }
            }
            for at in hits {
                self.player.mana = (self.player.mana + 12.0).min(player::MAX_MANA);
                self.hitstop = 0.045;
                self.shake = self.shake.max(1.5);
                self.burst(at, 8, PKind::Spark, GOLD_LIGHT, 110.0, 200.0, 0.35);
            }
        }

        // Projectiles against whoever they're aimed at.
        let mut sparks = Vec::new();
        for p in &mut self.projectiles {
            if p.dead {
                continue;
            }
            let r = p.rect();
            if p.friendly {
                if let Some(e) = self.enemies.iter_mut().find(|e| e.alive() && e.rect().overlaps(&r)) {
                    e.hit(p.dmg as i32, p.vel.x.signum());
                    p.dead = true;
                    sparks.push((p.pos, ROSE));
                }
            } else if self.player.rect().overlaps(&r) && self.player.hurt(p.dmg, p.pos.x) {
                if p.kind == ProjKind::Thorn {
                    p.dead = true;
                }
                sparks.push((p.pos, CRIMSON_LIGHT));
                self.shake = self.shake.max(4.0);
                self.hitstop = 0.06;
            }
        }
        for (at, color) in sparks {
            self.burst(at, 8, PKind::Spark, color, 90.0, 150.0, 0.4);
        }
        self.projectiles.retain(|p| !p.dead);

        // Enemy weapons and bodies against the player.
        let me = self.player.rect();
        let mut hurt_by = None;
        for e in &self.enemies {
            if !e.harmful() {
                continue;
            }
            if let Some((r, dmg)) = e.attack_box().filter(|(r, _)| r.overlaps(&me)) {
                hurt_by = Some((dmg, r.center().x));
            } else if e.rect().overlaps(&me) {
                hurt_by = Some((e.contact_damage(), e.center().x));
            }
        }
        if let Some((dmg, from)) = hurt_by
            && self.player.hurt(dmg, from)
        {
            self.shake = self.shake.max(4.0);
            self.hitstop = 0.06;
            self.burst(self.player.center(), 10, PKind::Spark, CRIMSON_LIGHT, 100.0, 150.0, 0.45);
        }

        // Fallen enemies scatter into gold and petals.
        let fallen: Vec<(Vec2, EnemyKind)> =
            self.enemies.iter().filter(|e| e.dead).map(|e| (e.center(), e.kind)).collect();
        for (at, kind) in fallen {
            let color = if kind == EnemyKind::Moth { MOTH } else { GOLD };
            self.burst(at, 16, PKind::Spark, color, 120.0, 160.0, 0.6);
            self.burst(at, 8, PKind::Petal, ROSE_DARK, 60.0, 50.0, 1.2);
            self.shake = self.shake.max(2.5);
        }
        self.enemies.retain(|e| !e.dead);
    }

    fn update_fight(&mut self, dt: f32) {
        if self.room.id != RoomId::Masquerade || self.flags.boss_defeated {
            return;
        }
        match self.fight {
            Fight::None => {
                let on_floor = self.player.on_ground && self.player.feet().y >= 15.0 * TILE - 1.0;
                if on_floor && self.player.pos.x > 6.0 * TILE {
                    self.fight = Fight::Intro(0.0);
                    self.room.sealed = true;
                    self.shake = 3.0;
                }
            }
            Fight::Intro(t) => {
                self.fight = Fight::Intro(t + dt);
                if t > BOSS_INTRO_TIME {
                    self.fight = Fight::Active;
                    self.enemies.iter_mut().for_each(Enemy::wake);
                }
            }
            Fight::Active => {
                let Some(boss) = self.boss() else { return };
                if boss.ai == Ai::Dying {
                    let (at, t) = (boss.center(), boss.t);
                    if self.rng.unit() < 0.5 {
                        let jitter = vec2(self.rng.range(-14.0, 14.0), self.rng.range(-24.0, 24.0));
                        self.burst(at + jitter, 2, PKind::Petal, ROSE, 70.0, -20.0, 1.4);
                    }
                    if t > MARQUISE_DEATH_TIME {
                        self.flags.boss_defeated = true;
                        self.room.sealed = false;
                        self.enemies.retain(|e| e.kind != EnemyKind::Marquise);
                        self.burst(at, 40, PKind::Petal, ROSE, 140.0, 30.0, 2.0);
                        self.scene = Scene::Victory(0.0);
                    }
                }
            }
        }
    }

    fn check_exits(&mut self) {
        let c = self.player.center();
        let side = if c.x < 0.0 {
            Side::Left
        } else if c.x > self.room.px_w() {
            Side::Right
        } else {
            return;
        };
        let row = (c.y / TILE).floor() as i32;
        if let Some(exit) = self.room.exit(side, row) {
            self.transition = Some(Transition { t: 0.0, to: exit.to, arrive: exit.arrive, swapped: false });
        }
    }

    fn camera_target(&self) -> Vec2 {
        let c = self.player.center();
        let fit = |room: f32, view: f32, at: f32| {
            if room <= view { (room - view) / 2.0 } else { (at - view / 2.0).clamp(0.0, room - view) }
        };
        vec2(fit(self.room.px_w(), VIEW_W, c.x), fit(self.room.px_h(), VIEW_H, c.y - 10.0))
    }

    pub fn burst(&mut self, at: Vec2, n: usize, kind: PKind, color: Color, speed: f32, gravity: f32, life: f32) {
        for _ in 0..n {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let s = self.rng.range(0.3, 1.0) * speed;
            let life = life * self.rng.range(0.6, 1.0);
            self.particles.push(Particle {
                kind,
                pos: at,
                vel: vec2(a.cos(), a.sin()) * s,
                life,
                max_life: life,
                size: self.rng.range(1.0, 2.5),
                color,
                gravity,
                seed: self.rng.range(0.0, 100.0),
            });
        }
    }

    fn update_particles(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.life -= dt;
            p.vel.y += p.gravity * dt;
            if p.kind == PKind::Petal {
                p.vel.x += (p.life * 3.0 + p.seed).sin() * 30.0 * dt;
                p.vel *= 1.0 - 1.5 * dt;
            }
            p.pos += p.vel * dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    /// Drifting rose petals in the palace; dust motes in the library.
    fn spawn_ambient(&mut self, dt: f32) {
        let library = self.scene != Scene::Title && self.room.theme == Theme::Library;
        let rate = if library { 6.0 } else { 2.0 };
        self.ambient += dt * rate;
        while self.ambient >= 1.0 && self.particles.len() < 400 {
            self.ambient -= 1.0;
            let x = self.cam.x + self.rng.range(-20.0, VIEW_W + 20.0);
            let (kind, pos, vel, color, life) = if library {
                let y = self.cam.y + self.rng.range(0.0, VIEW_H);
                let vel = vec2(self.rng.range(-4.0, 4.0), self.rng.range(-6.0, -1.0));
                (PKind::Mote, vec2(x, y), vel, with_alpha(CRYSTAL, 0.5), 5.0)
            } else {
                let vel = vec2(self.rng.range(-12.0, 6.0), self.rng.range(12.0, 22.0));
                (PKind::Petal, vec2(x, self.cam.y - 6.0), vel, with_alpha(ROSE, 0.85), 14.0)
            };
            self.particles.push(Particle {
                kind,
                pos,
                vel,
                life,
                max_life: life,
                size: self.rng.range(1.0, 2.0),
                color,
                gravity: 0.0,
                seed: self.rng.range(0.0, 100.0),
            });
        }
        self.ambient = self.ambient.min(1.0);
    }

    /// Sets up a named moment for `--shot`. Returns fixed steps to simulate first.
    pub fn debug_scene(&mut self, name: &str) -> usize {
        let at = |col: f32, row: f32| vec2(col * TILE, row * TILE);
        match name {
            "title" => 240,
            "gallery" => {
                self.start();
                60
            }
            "fight" => {
                self.start();
                self.player.place(at(19.5, 15.0));
                40
            }
            "library" => {
                self.start();
                self.load_room(RoomId::Library, at(16.5, 15.0));
                90
            }
            "letter" => {
                self.debug_scene("library");
                self.scene = Scene::Popup(Popup::Letter);
                90
            }
            "relic" => {
                self.start();
                self.load_room(RoomId::Library, at(5.5, 6.0));
                90
            }
            "ledge" => {
                self.start();
                self.flags.ribbon = true;
                self.player.place(at(43.5, 6.0));
                60
            }
            "boss" => {
                self.start();
                self.flags.ribbon = true;
                self.load_room(RoomId::Masquerade, at(9.5, 15.0));
                self.banner = None;
                420
            }
            "victory" => {
                self.start();
                self.scene = Scene::Victory(5.0);
                10
            }
            "dead" => {
                self.start();
                self.scene = Scene::Dead(2.0);
                10
            }
            other => panic!("unknown scene {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::PW;
    use crate::world::ALL_ROOMS;

    const DT: f32 = 1.0 / 120.0;

    /// Jumps from `feet` while holding a direction; with `air_jump_at` a
    /// second jump is pressed on that frame. Returns where the player first
    /// lands, if anywhere other than the starting height.
    fn jump(room: &Room, feet: Vec2, dir: f32, ribbon: bool, air_jump_at: Option<usize>) -> Option<Vec2> {
        let mut p = Player::new(feet);
        p.update(&Input::default(), room, ribbon, DT);
        for frame in 0..240 {
            let inp = Input {
                left: dir < 0.0,
                right: dir > 0.0,
                jump_held: true,
                jump: frame == 0 || Some(frame) == air_jump_at,
                ..Default::default()
            };
            p.update(&inp, room, ribbon, DT);
            if frame > 5 && p.on_ground {
                return Some(p.feet());
            }
        }
        None
    }

    #[test]
    fn rooms_parse_and_exits_pair_up() {
        for id in ALL_ROOMS {
            let room = world::build(id);
            for exit in &room.exits {
                let target = world::build(exit.to);
                let back = target.exits.iter().find(|e| e.to == id).expect("exits are two-way");
                assert_ne!(back.side, exit.side, "{id:?} -> {:?} arrives on the facing side", exit.to);
                let arrive = Player::new(exit.arrive).rect();
                for (tx, ty) in [(arrive.x, arrive.y), (arrive.right() - 0.1, arrive.bottom() - 0.1)] {
                    assert_ne!(
                        target.tile_at(vec2(tx, ty)),
                        Tile::Solid,
                        "{id:?} -> {:?} arrives inside a wall",
                        exit.to
                    );
                }
            }
        }
    }

    #[test]
    fn gallery_ledge_needs_the_ribbon() {
        let room = world::build(RoomId::Gallery);
        let balcony = vec2(37.5 * TILE, 12.0 * TILE);
        let ledge_top = 6.0 * TILE;
        let single = jump(&room, balcony, 1.0, false, None);
        assert!(single.is_none_or(|f| f.y > ledge_top), "a single jump reached the ledge: {single:?}");
        let double = jump(&room, balcony, 1.0, true, Some(36)).expect("lands somewhere");
        assert_eq!(double.y, ledge_top, "a double jump should reach the ledge");
    }

    #[test]
    fn library_balconies_reach_the_relic_with_single_jumps() {
        let room = world::build(RoomId::Library);
        let steps = [(37.5, 15.0, 12.0), (30.5, 12.0, 9.0), (22.5, 9.0, 6.0)];
        for (col, row, next_row) in steps {
            let landed = jump(&room, vec2(col * TILE, row * TILE), -1.0, false, None);
            assert_eq!(landed.map(|f| f.y), Some(next_row * TILE), "from col {col} row {row}");
        }
    }

    #[test]
    fn a_long_random_session_never_panics() {
        let mut g = Game::new();
        g.start();
        g.flags.ribbon = true;
        let mut rng = Rng::new(7);
        for frame in 0..120 * 90 {
            if frame % 2400 == 0 {
                let rooms = [RoomId::Gallery, RoomId::Library, RoomId::Masquerade];
                let room = rooms[(frame / 2400) % 3];
                let start = world::build(room).exits[0].arrive;
                g.load_room(room, start);
                g.scene = Scene::Playing;
            }
            let inp = Input {
                left: rng.unit() < 0.3,
                right: rng.unit() < 0.5,
                jump_held: rng.unit() < 0.5,
                jump: rng.unit() < 0.05,
                attack: rng.unit() < 0.1,
                cast: rng.unit() < 0.02,
                heal: rng.unit() < 0.005,
                interact: rng.unit() < 0.02,
                confirm: rng.unit() < 0.02,
                down: rng.unit() < 0.05,
            };
            g.update(DT, inp);
            assert!(g.player.pos.is_finite());
        }
    }

    #[test]
    fn the_marquise_can_be_defeated() {
        let mut g = Game::new();
        g.start();
        g.flags.ribbon = true;
        g.load_room(RoomId::Masquerade, vec2(9.5 * TILE, 15.0 * TILE));
        // Let the intro play, then whittle her down with an invulnerable heroine.
        for _ in 0..400 {
            g.update(DT, Input::default());
        }
        assert_eq!(g.fight, Fight::Active);
        assert!(g.room.sealed, "the hall seals during the duel");
        for frame in 0..120 * 60 {
            g.player.invuln = 1.0;
            g.player.hp = player::MAX_HP;
            if let Some(her) = g.boss().map(Enemy::center) {
                // Stand on whichever side of her has more room, facing her.
                let side = if her.x < g.room.px_w() / 2.0 { 1.0 } else { -1.0 };
                g.player.facing = -side;
                g.player.pos.x = her.x + side * 20.0 - PW / 2.0;
            }
            g.update(DT, Input { attack: frame % 20 == 0, ..Default::default() });
            if g.scene != Scene::Playing {
                break;
            }
        }
        assert!(matches!(g.scene, Scene::Victory(_)), "ended in {:?}", g.scene);
        assert!(g.flags.boss_defeated && !g.room.sealed);
    }
}

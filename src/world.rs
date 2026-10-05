//! Rooms, tiles and tile collision.
//!
//! Rooms are authored as ASCII maps. Each character is one 16px tile:
//!
//! | char | meaning                                  |
//! |------|------------------------------------------|
//! | `#`  | solid marble / shelving                  |
//! | `=`  | one-way gilded balcony (drop with ↓+jump) |
//! | `C`  | chandelier hanging from this cell        |
//! | `c`  | standing candelabra                      |
//! | `l`  | hanging lantern                          |
//! | `P`  | player start                             |
//! | `H`  | halberdier guard                         |
//! | `m`  | moth                                     |
//! | `B`  | the Masked Marquise                      |
//! | `R`  | relic pedestal (the silk ribbon)         |
//! | `L`  | writing desk with a letter               |

use macroquad::prelude::*;

pub const TILE: f32 = 16.0;
const EPS: f32 = 0.001;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    Empty,
    Solid,
    OneWay,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoomId {
    Gallery,
    Library,
    Masquerade,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Palace,
    Library,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

/// An opening in the left or right edge of a room.
#[derive(Clone, Copy, Debug)]
pub struct Exit {
    pub side: Side,
    /// Inclusive tile rows the opening spans.
    pub rows: (i32, i32),
    pub to: RoomId,
    /// Where the player's feet land in the target room, in pixels.
    pub arrive: Vec2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecorKind {
    Chandelier,
    Candelabra,
    Lantern,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpawnKind {
    Player,
    Halberdier,
    Moth,
    Marquise,
    Relic,
    Letter,
}

/// Something placed in a map cell. `pos` is the bottom-centre of the cell,
/// which is where a standing thing's feet go.
#[derive(Clone, Copy, Debug)]
pub struct Placed<K> {
    pub kind: K,
    pub pos: Vec2,
}

pub struct Room {
    pub id: RoomId,
    pub name: &'static str,
    pub theme: Theme,
    pub w: i32,
    pub h: i32,
    tiles: Vec<Tile>,
    pub decor: Vec<Placed<DecorKind>>,
    pub spawns: Vec<Placed<SpawnKind>>,
    pub exits: Vec<Exit>,
    /// While sealed (during the boss fight) exits behave like walls.
    pub sealed: bool,
}

impl Room {
    fn parse(id: RoomId, name: &'static str, theme: Theme, rows: &[&str], exits: Vec<Exit>) -> Room {
        let h = rows.len() as i32;
        let w = rows[0].chars().count() as i32;
        let mut room = Room {
            id,
            name,
            theme,
            w,
            h,
            tiles: Vec::with_capacity((w * h) as usize),
            decor: Vec::new(),
            spawns: Vec::new(),
            exits,
            sealed: false,
        };
        for (y, row) in rows.iter().enumerate() {
            assert_eq!(row.chars().count() as i32, w, "{name}: row {y} has the wrong width");
            for (x, ch) in row.chars().enumerate() {
                let pos = vec2(x as f32 * TILE + TILE / 2.0, (y + 1) as f32 * TILE);
                let tile = match ch {
                    '#' => Tile::Solid,
                    '=' => Tile::OneWay,
                    _ => Tile::Empty,
                };
                room.tiles.push(tile);
                let decor = |kind| Some(Placed { kind, pos });
                let spawn = |kind| Some(Placed { kind, pos });
                let (d, s) = match ch {
                    '#' | '=' | '.' => (None, None),
                    'C' => (decor(DecorKind::Chandelier), None),
                    'c' => (decor(DecorKind::Candelabra), None),
                    'l' => (decor(DecorKind::Lantern), None),
                    'P' => (None, spawn(SpawnKind::Player)),
                    'H' => (None, spawn(SpawnKind::Halberdier)),
                    'm' => (None, spawn(SpawnKind::Moth)),
                    'B' => (None, spawn(SpawnKind::Marquise)),
                    'R' => (None, spawn(SpawnKind::Relic)),
                    'L' => (None, spawn(SpawnKind::Letter)),
                    other => panic!("{name}: unknown map character {other:?} at ({x}, {y})"),
                };
                room.decor.extend(d);
                room.spawns.extend(s);
            }
        }
        room
    }

    pub fn px_w(&self) -> f32 {
        self.w as f32 * TILE
    }

    pub fn px_h(&self) -> f32 {
        self.h as f32 * TILE
    }

    /// The tile at a grid cell. Outside the map is solid, except through an
    /// open exit on the left or right edge.
    pub fn tile(&self, tx: i32, ty: i32) -> Tile {
        if ty < 0 || ty >= self.h {
            return Tile::Solid;
        }
        if tx < 0 || tx >= self.w {
            let side = if tx < 0 { Side::Left } else { Side::Right };
            return if self.exit(side, ty).is_some() { Tile::Empty } else { Tile::Solid };
        }
        self.tiles[(ty * self.w + tx) as usize]
    }

    pub fn tile_at(&self, p: Vec2) -> Tile {
        self.tile((p.x / TILE).floor() as i32, (p.y / TILE).floor() as i32)
    }

    /// The open exit on `side` covering `row`, if any.
    pub fn exit(&self, side: Side, row: i32) -> Option<&Exit> {
        if self.sealed {
            return None;
        }
        self.exits.iter().find(|e| e.side == side && (e.rows.0..=e.rows.1).contains(&row))
    }

    pub fn spawn(&self, kind: SpawnKind) -> Option<Vec2> {
        self.spawns.iter().find(|s| s.kind == kind).map(|s| s.pos)
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Contacts {
    pub ground: bool,
    pub wall: bool,
    pub ceiling: bool,
}

fn span(start: f32, len: f32) -> std::ops::RangeInclusive<i32> {
    (start / TILE).floor() as i32..=((start + len - EPS) / TILE).floor() as i32
}

/// Moves an axis-aligned box through the room, resolving X then Y.
/// One-way tiles only catch a box falling onto them from above, and not
/// at all while `drop_through` is set.
pub fn move_body(room: &Room, pos: &mut Vec2, size: Vec2, vel: &mut Vec2, dt: f32, drop_through: bool) -> Contacts {
    let mut c = Contacts::default();

    pos.x += vel.x * dt;
    if vel.x != 0.0 {
        let rows = span(pos.y, size.y);
        let tx =
            if vel.x > 0.0 { ((pos.x + size.x - EPS) / TILE).floor() as i32 } else { (pos.x / TILE).floor() as i32 };
        if rows.clone().any(|ty| room.tile(tx, ty) == Tile::Solid) {
            pos.x = if vel.x > 0.0 { tx as f32 * TILE - size.x } else { (tx + 1) as f32 * TILE };
            vel.x = 0.0;
            c.wall = true;
        }
    }

    let prev_bottom = pos.y + size.y;
    pos.y += vel.y * dt;
    let cols = span(pos.x, size.x);
    if vel.y > 0.0 {
        let ty = ((pos.y + size.y - EPS) / TILE).floor() as i32;
        let top = ty as f32 * TILE;
        let blocked = cols.clone().any(|tx| match room.tile(tx, ty) {
            Tile::Solid => true,
            Tile::OneWay => !drop_through && prev_bottom <= top + 0.5,
            Tile::Empty => false,
        });
        if blocked {
            pos.y = top - size.y;
            vel.y = 0.0;
            c.ground = true;
        }
    } else if vel.y < 0.0 {
        let ty = (pos.y / TILE).floor() as i32;
        if cols.clone().any(|tx| room.tile(tx, ty) == Tile::Solid) {
            pos.y = (ty + 1) as f32 * TILE;
            vel.y = 0.0;
            c.ceiling = true;
        }
    }
    c
}

/// True when the box is standing on one-way tiles and nothing solid.
pub fn standing_on_one_way(room: &Room, pos: Vec2, size: Vec2) -> bool {
    let ty = ((pos.y + size.y + 0.5) / TILE).floor() as i32;
    let tiles: Vec<Tile> = span(pos.x, size.x).map(|tx| room.tile(tx, ty)).collect();
    tiles.contains(&Tile::OneWay) && !tiles.contains(&Tile::Solid)
}

pub fn approach(current: f32, target: f32, step: f32) -> f32 {
    if current < target { (current + step).min(target) } else { (current - step).max(target) }
}

const GALLERY: [&str; 17] = [
    "################################################",
    "#.....C...........C...........C................#",
    "#..............................................#",
    "#...............................................",
    "#...............................................",
    "#...............................................",
    "#.......................................########",
    "#.......................................########",
    "#.......................................########",
    "#.......................................########",
    "#.......................................########",
    "#.......................................########",
    "...........=====.................======.########",
    "........................................########",
    "...c....P....c.........H........c.......########",
    "################################################",
    "################################################",
];

const LIBRARY: [&str; 17] = [
    "############################################",
    "#.........l...............l...........l....#",
    "#..........................................#",
    "#..........................................#",
    "#..........................................#",
    "#...R......................................#",
    "#########===========.......................#",
    "#...................m......................#",
    "#..........................................#",
    "#.....................======...............#",
    "#..........m...............................#",
    "#..........................................#",
    "#.............................======........",
    "#..............................m............",
    "#.....c......L..........c...............c...",
    "############################################",
    "############################################",
];

const MASQUERADE: [&str; 17] = [
    "##############################",
    "#......C.......C......C......#",
    "#............................#",
    ".............................#",
    ".............................#",
    ".............................#",
    "#####........................#",
    "#............................#",
    "#............................#",
    "#............................#",
    "#............................#",
    "#.......====......====.......#",
    "#............................#",
    "#............................#",
    "#..c..................B...c..#",
    "##############################",
    "##############################",
];

pub fn build(id: RoomId) -> Room {
    let at = |col: f32, row: f32| vec2(col * TILE, row * TILE);
    match id {
        RoomId::Gallery => Room::parse(
            id,
            "The Grand Gallery",
            Theme::Palace,
            &GALLERY,
            vec![
                Exit { side: Side::Left, rows: (12, 14), to: RoomId::Library, arrive: at(42.5, 15.0) },
                Exit { side: Side::Right, rows: (3, 5), to: RoomId::Masquerade, arrive: at(1.5, 6.0) },
            ],
        ),
        RoomId::Library => Room::parse(
            id,
            "The Whispering Library",
            Theme::Library,
            &LIBRARY,
            vec![Exit { side: Side::Right, rows: (12, 14), to: RoomId::Gallery, arrive: at(1.5, 15.0) }],
        ),
        RoomId::Masquerade => Room::parse(
            id,
            "Hall of the Endless Masque",
            Theme::Palace,
            &MASQUERADE,
            vec![Exit { side: Side::Left, rows: (3, 5), to: RoomId::Gallery, arrive: at(46.5, 6.0) }],
        ),
    }
}

#[cfg(test)]
pub const ALL_ROOMS: [RoomId; 3] = [RoomId::Gallery, RoomId::Library, RoomId::Masquerade];

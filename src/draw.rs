//! Renders the world into the low-resolution render target. Everything
//! except the two painted backdrops is drawn from primitives here.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation};
use macroquad::prelude::*;

use crate::enemy::{Ai, Enemy, EnemyKind, HALBERD_WINDUP, MARQUISE_DEATH_TIME};
use crate::game::{Fight, Game, InterKind, PKind, ProjKind, Scene};
use crate::palette::GOLD;
use crate::palette::*;
use crate::player::{self, ATTACK_TIME, Player};
use crate::world::{DecorKind, Room, RoomId, Side, TILE, Theme, Tile};
use crate::{VIEW_H, VIEW_W};

pub struct Assets {
    /// Copies the view to the window ignoring its alpha, which translucent
    /// drawing inside the render target leaves below 1.
    pub opaque: Material,
    gallery: Texture2D,
    library: Texture2D,
    glow: Texture2D,
    vignette: Texture2D,
    additive: Material,
    pub display: Font,
    pub body: Font,
}

const GLOW_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;

const GLOW_FRAGMENT: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

const OPAQUE_FRAGMENT: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = vec4((color * texture2D(Texture, uv)).rgb, 1.0);
}"#;

impl Assets {
    pub fn load() -> Self {
        let texture = |bytes: &[u8]| {
            let t = Texture2D::from_file_with_format(bytes, None);
            t.set_filter(FilterMode::Linear);
            t
        };
        let font = |bytes: &[u8]| {
            let mut f = load_ttf_font_from_bytes(bytes).expect("bundled font loads");
            f.set_filter(FilterMode::Linear);
            f
        };
        let additive = load_material(
            ShaderSource::Glsl { vertex: GLOW_VERTEX, fragment: GLOW_FRAGMENT },
            MaterialParams {
                pipeline_params: PipelineParams {
                    color_blend: Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::One,
                    )),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .expect("glow shader compiles");
        let opaque = load_material(
            ShaderSource::Glsl { vertex: GLOW_VERTEX, fragment: OPAQUE_FRAGMENT },
            MaterialParams::default(),
        )
        .expect("blit shader compiles");
        Self {
            opaque,
            gallery: texture(include_bytes!("../assets/backdrops/gallery.png")),
            library: texture(include_bytes!("../assets/backdrops/library.png")),
            glow: radial_texture(64, WHITE, |d| (1.0 - d).powf(1.6)),
            vignette: radial_texture(96, BLACK, |d| (d * d * 1.1).min(1.0) * 0.8),
            additive,
            display: font(include_bytes!("../assets/fonts/Cinzel.ttf")),
            body: font(include_bytes!("../assets/fonts/EBGaramond.ttf")),
        }
    }
}

/// A texture of `color` whose alpha is `alpha(distance from centre, 0..1)`.
fn radial_texture(size: u16, color: Color, alpha: impl Fn(f32) -> f32) -> Texture2D {
    let mut img = Image::gen_image_color(size, size, Color::new(1.0, 1.0, 1.0, 0.0));
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size as u32 {
        for x in 0..size as u32 {
            let d = ((x as f32 - c).hypot(y as f32 - c) / c).min(1.0);
            img.set_pixel(x, y, with_alpha(color, alpha(d)));
        }
    }
    let t = Texture2D::from_image(&img);
    t.set_filter(FilterMode::Linear);
    t
}

/// Additive light, drawn after everything else in the world.
struct Glow {
    at: Vec2,
    radius: f32,
    color: Color,
}

type Glows = Vec<Glow>;

fn glow(glows: &mut Glows, at: Vec2, radius: f32, color: Color) {
    glows.push(Glow { at, radius, color });
}

fn flicker(t: f32, seed: f32) -> f32 {
    0.85 + 0.15 * (t * 13.0 + seed).sin() * (t * 7.3 + seed * 2.0).sin()
}

fn hash(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

/// The camera's top-left this frame, including shake, snapped to pixels.
pub fn camera(g: &Game) -> Vec2 {
    let shake = if g.shake > 0.0 { vec2((g.time * 97.0).sin(), (g.time * 71.0).cos()) * g.shake } else { Vec2::ZERO };
    (g.cam + shake).round()
}

pub fn world(g: &Game, a: &Assets, rt: &RenderTarget) {
    let cam = camera(g);
    let mut c = Camera2D::from_display_rect(Rect::new(cam.x, cam.y, VIEW_W, VIEW_H));
    c.render_target = Some(rt.clone());
    set_camera(&c);
    clear_background(INK);

    let mut glows = Glows::new();
    if g.scene == Scene::Title {
        title_backdrop(g, a, cam);
    } else {
        backdrop(g, a, cam);
        decor(g, &mut glows);
        exits(&g.room, &mut glows);
        tiles(&g.room, cam);
        interactables(g, &mut glows);
        for e in &g.enemies {
            enemy(e, g.time, &mut glows);
        }
        if g.player.hp > 0.0 {
            heroine(&g.player, g.flags.ribbon, &mut glows);
        }
        projectiles(g, &mut glows);
    }
    particles(g);

    gl_use_material(&a.additive);
    for gl in &glows {
        let size = vec2(gl.radius, gl.radius) * 2.0;
        let params = DrawTextureParams { dest_size: Some(size), ..Default::default() };
        draw_texture_ex(&a.glow, gl.at.x - gl.radius, gl.at.y - gl.radius, gl.color, params);
    }
    gl_use_default_material();

    let params = DrawTextureParams { dest_size: Some(vec2(VIEW_W, VIEW_H)), ..Default::default() };
    draw_texture_ex(&a.vignette, cam.x, cam.y, WHITE, params);

    if matches!(g.scene, Scene::Playing | Scene::Popup(_)) {
        let mut hud_cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, VIEW_W, VIEW_H));
        hud_cam.render_target = Some(rt.clone());
        set_camera(&hud_cam);
        hud(g);
    }
}

fn title_backdrop(g: &Game, a: &Assets, cam: Vec2) {
    let pan = (g.time * 0.05).sin() * 20.0;
    let size = vec2(VIEW_W, VIEW_H) * 1.15;
    let params = DrawTextureParams { dest_size: Some(size), ..Default::default() };
    let tint = Color::new(0.42, 0.36, 0.42, 1.0);
    draw_texture_ex(&a.gallery, cam.x - 36.0 + pan, cam.y - 20.0, tint, params);
    draw_rectangle(cam.x, cam.y, VIEW_W, VIEW_H, with_alpha(INK, 0.35));
}

fn backdrop(g: &Game, a: &Assets, cam: Vec2) {
    let room = &g.room;
    let (tex, tint) = match room.id {
        RoomId::Gallery => (&a.gallery, Color::new(0.68, 0.6, 0.66, 1.0)),
        RoomId::Masquerade => (&a.gallery, Color::new(0.82, 0.48, 0.56, 1.0)),
        RoomId::Library => (&a.library, Color::new(0.95, 0.95, 1.05, 1.0)),
    };
    let size = vec2(VIEW_W, VIEW_H) * 1.3;
    let travel = (room.px_w() - VIEW_W).max(1.0);
    let fx = if room.px_w() > VIEW_W { cam.x / travel } else { 0.5 };
    let x = cam.x - fx * (size.x - VIEW_W);
    let y = cam.y - (size.y - VIEW_H) * 0.3;
    let params = DrawTextureParams { dest_size: Some(size), ..Default::default() };
    draw_texture_ex(tex, x, y, tint, params);
    draw_rectangle(cam.x, cam.y, VIEW_W, VIEW_H, Color::new(0.04, 0.03, 0.08, 0.28));
}

fn tiles(room: &Room, cam: Vec2) {
    let x0 = (cam.x / TILE).floor() as i32 - 1;
    let y0 = (cam.y / TILE).floor() as i32 - 1;
    let x1 = ((cam.x + VIEW_W) / TILE).ceil() as i32 + 1;
    let y1 = ((cam.y + VIEW_H) / TILE).ceil() as i32 + 1;
    for ty in y0.max(0)..y1.min(room.h) {
        for tx in x0.max(0)..x1.min(room.w) {
            let (x, y) = (tx as f32 * TILE, ty as f32 * TILE);
            match (room.tile(tx, ty), room.theme) {
                (Tile::Solid, Theme::Palace) => palace_tile(room, tx, ty, x, y),
                (Tile::Solid, Theme::Library) => library_tile(room, tx, ty, x, y),
                (Tile::OneWay, theme) => balcony(theme, x, y),
                (Tile::Empty, _) => {}
            }
        }
    }
    if room.theme == Theme::Palace {
        for ty in y0.max(0)..y1.min(room.h) {
            for tx in x0.max(0)..x1.min(room.w) {
                boiserie(room, tx, ty);
            }
        }
    }
}

/// Deep palace walls become gilded boiserie: a framed panel per 2×2 block
/// of fully enclosed tiles. Drawn after all tiles so neighbours can't cover it.
fn boiserie(room: &Room, tx: i32, ty: i32) {
    let enclosed = |dx: i32, dy: i32| {
        room.tile(tx + dx, ty + dy) == Tile::Solid && exposed(room, tx + dx, ty + dy).iter().all(|e| !e)
    };
    if tx % 2 != 0 || ty % 2 != 0 || ![(0, 0), (1, 0), (0, 1), (1, 1)].iter().all(|&(dx, dy)| enclosed(dx, dy)) {
        return;
    }
    let (x, y) = (tx as f32 * TILE, ty as f32 * TILE);
    let size = TILE * 2.0 - 6.0;
    draw_rectangle(x + 3.0, y + 3.0, size, size, with_alpha(MARBLE_DARK, 0.6));
    draw_rectangle_lines(x + 3.0, y + 3.0, size, size, 1.0, with_alpha(GOLD_DARK, 0.6));
    draw_rectangle_lines(x + 5.0, y + 5.0, size - 4.0, size - 4.0, 1.0, with_alpha(GOLD_DEEP, 0.8));
    draw_poly(x + TILE, y + TILE, 4, 2.5, 0.0, with_alpha(GOLD_DARK, 0.6));
}

fn exposed(room: &Room, tx: i32, ty: i32) -> [bool; 4] {
    let open = |dx: i32, dy: i32| room.tile(tx + dx, ty + dy) != Tile::Solid;
    [open(0, -1), open(0, 1), open(-1, 0), open(1, 0)]
}

fn palace_tile(room: &Room, tx: i32, ty: i32, x: f32, y: f32) {
    let [up, down, left, right] = exposed(room, tx, ty);
    let interior = !(up || down || left || right);
    let base = match (interior, hash(tx, ty) % 5) {
        (true, _) => MARBLE_DEEP,
        (false, 0) => MARBLE_DARK,
        _ => MARBLE,
    };
    draw_rectangle(x, y, TILE, TILE, base);
    if up {
        let top = if (tx + ty) % 2 == 0 { MARBLE_TOP } else { MARBLE_TOP_DARK };
        draw_rectangle(x, y, TILE, 5.0, top);
        draw_rectangle(x, y, TILE, 1.0, with_alpha(GOLD_LIGHT, 0.7));
        draw_rectangle(x, y + 5.0, TILE, 1.0, GOLD);
        draw_rectangle(x, y + 6.0, TILE, 1.0, GOLD_DEEP);
    }
    if down {
        draw_rectangle(x, y + TILE - 2.0, TILE, 2.0, GOLD_DARK);
    }
    if left {
        draw_rectangle(x, y, 1.0, TILE, GOLD_DARK);
        draw_rectangle(x + 1.0, y, 1.0, TILE, with_alpha(MARBLE_TOP_DARK, 0.4));
    }
    if right {
        draw_rectangle(x + TILE - 1.0, y, 1.0, TILE, GOLD_DARK);
    }
}

fn library_tile(room: &Room, tx: i32, ty: i32, x: f32, y: f32) {
    let [up, down, left, right] = exposed(room, tx, ty);
    let h = hash(tx, ty);
    draw_rectangle(x, y, TILE, TILE, WOOD_DARK);
    if ty == 0 || ty >= room.h - 2 {
        draw_rectangle(x, y + 7.0, TILE, 1.0, WOOD_DEEP);
        draw_rectangle(x + (h % 16) as f32, y, 1.0, 7.0, WOOD_DEEP);
        draw_rectangle(x + ((h >> 4) % 16) as f32, y + 8.0, 1.0, 8.0, WOOD_DEEP);
    } else {
        // A shelf of book spines.
        let mut bx = x + 1.0;
        let mut i = 0;
        while bx < x + TILE - 2.0 {
            let bits = h.rotate_right(i * 5);
            let w = 2.0 + (bits % 2) as f32;
            let tall = 9.0 + ((bits >> 1) % 4) as f32;
            let col = BOOKS[((bits >> 3) as usize) % BOOKS.len()];
            draw_rectangle(bx, y + 14.0 - tall, w, tall, col);
            draw_rectangle(bx, y + 15.0 - tall, w, 1.0, with_alpha(GOLD, 0.35));
            bx += w + if bits.is_multiple_of(7) { 1.0 } else { 0.0 };
            i += 1;
        }
        draw_rectangle(x, y + 14.0, TILE, 2.0, WOOD);
    }
    if up {
        draw_rectangle(x, y, TILE, 3.0, WOOD_LIGHT);
        draw_rectangle(x, y + 3.0, TILE, 1.0, GOLD_DARK);
    }
    if down {
        draw_rectangle(x, y + TILE - 1.0, TILE, 1.0, GOLD_DARK);
    }
    if left {
        draw_rectangle(x, y, 1.0, TILE, GOLD_DARK);
    }
    if right {
        draw_rectangle(x + TILE - 1.0, y, 1.0, TILE, GOLD_DARK);
    }
}

fn balcony(theme: Theme, x: f32, y: f32) {
    let (rail, shine, post, base) = match theme {
        Theme::Palace => (GOLD, GOLD_LIGHT, GOLD_DARK, GOLD_DEEP),
        Theme::Library => (WOOD_LIGHT, GOLD, WOOD, WOOD_DEEP),
    };
    draw_rectangle(x, y, TILE, 3.0, rail);
    draw_rectangle(x, y, TILE, 1.0, shine);
    for i in 0..4 {
        let bx = x + 1.0 + i as f32 * 4.0;
        draw_rectangle(bx, y + 3.0, 2.0, 8.0, post);
        draw_rectangle(bx - 0.5, y + 6.0, 3.0, 2.0, post);
    }
    draw_rectangle(x, y + 11.0, TILE, 2.0, base);
}

fn candle(x: f32, y: f32, t: f32, seed: f32, glows: &mut Glows) {
    let f = flicker(t, seed);
    draw_rectangle(x - 1.0, y, 2.0, 3.0, CREAM);
    draw_circle(x, y - 1.8, 1.6 * f, CANDLE);
    draw_rectangle(x - 0.5, y - 2.5, 1.0, 1.5, FLAME_CORE);
    glow(glows, vec2(x, y - 2.0), 10.0 * f, Color::new(1.0, 0.75, 0.4, 0.8));
}

fn decor(g: &Game, glows: &mut Glows) {
    let t = g.time;
    for d in &g.room.decor {
        let (cx, seed) = (d.pos.x, d.pos.x * 0.13);
        match d.kind {
            DecorKind::Chandelier => {
                let top = d.pos.y - TILE;
                let cy = top + 26.0;
                draw_line(cx, top, cx, cy - 6.0, 1.0, GOLD_DARK);
                draw_circle(cx, cy - 6.0, 2.0, GOLD);
                for side in [-1.0, 1.0] {
                    for (i, reach) in [8.0, 15.0].into_iter().enumerate() {
                        let ex = cx + side * reach;
                        let ey = cy - 3.0 - i as f32 * 2.0;
                        draw_line(cx, cy - 3.0, ex, cy + 2.0, 1.0, GOLD);
                        draw_line(ex, cy + 2.0, ex, ey + 3.0, 1.0, GOLD);
                        candle(ex, ey, t, seed + side * 3.0 + reach, glows);
                    }
                }
                candle(cx, cy - 11.0, t, seed, glows);
                for i in -3i32..=3 {
                    let px = cx + i as f32 * 3.5;
                    let len = 4.0 + (i % 2).abs() as f32 * 2.0;
                    draw_line(px, cy + 2.0, px, cy + 2.0 + len, 1.0, with_alpha(CRYSTAL, 0.7));
                    draw_circle(px, cy + 2.5 + len, 0.9, CRYSTAL);
                }
                draw_rectangle(cx - 1.0, cy, 2.0, 9.0, GOLD_DARK);
                draw_circle(cx, cy + 10.0, 1.5, GOLD);
                glow(glows, vec2(cx, cy - 2.0), 80.0, Color::new(1.0, 0.68, 0.38, 0.42 * flicker(t, seed)));
            }
            DecorKind::Candelabra => {
                let fy = d.pos.y;
                draw_rectangle(cx - 4.0, fy - 2.0, 8.0, 2.0, GOLD_DARK);
                draw_rectangle(cx - 1.0, fy - 22.0, 2.0, 20.0, GOLD_DARK);
                draw_circle(cx, fy - 12.0, 2.0, GOLD);
                draw_line(cx - 7.0, fy - 22.0, cx + 7.0, fy - 22.0, 1.0, GOLD);
                draw_line(cx - 7.0, fy - 22.0, cx - 7.0, fy - 25.0, 1.0, GOLD);
                draw_line(cx + 7.0, fy - 22.0, cx + 7.0, fy - 25.0, 1.0, GOLD);
                candle(cx - 7.0, fy - 28.0, t, seed, glows);
                candle(cx, fy - 31.0, t, seed + 1.7, glows);
                candle(cx + 7.0, fy - 28.0, t, seed + 3.1, glows);
                glow(glows, vec2(cx, fy - 28.0), 50.0, Color::new(1.0, 0.66, 0.36, 0.45 * flicker(t, seed)));
            }
            DecorKind::Lantern => {
                let top = d.pos.y - TILE;
                let y = top + 16.0;
                let f = flicker(t, seed);
                draw_line(cx, top, cx, y, 1.0, GOLD_DARK);
                draw_triangle(vec2(cx - 4.0, y + 1.0), vec2(cx + 4.0, y + 1.0), vec2(cx, y - 3.0), GOLD_DARK);
                draw_rectangle(cx - 3.0, y + 1.0, 6.0, 9.0, Color::new(1.0, 0.68, 0.32, 0.85 * f));
                draw_rectangle_lines(cx - 3.0, y + 1.0, 6.0, 9.0, 1.0, GOLD_DARK);
                draw_line(cx, y + 1.0, cx, y + 10.0, 1.0, GOLD_DARK);
                draw_triangle(vec2(cx - 3.0, y + 10.0), vec2(cx + 3.0, y + 10.0), vec2(cx, y + 13.0), GOLD_DARK);
                glow(glows, vec2(cx, y + 5.0), 64.0, Color::new(1.0, 0.62, 0.3, 0.5 * f));
            }
        }
    }
}

fn exits(room: &Room, glows: &mut Glows) {
    for e in &room.exits {
        let (x, inward) = match e.side {
            Side::Left => (0.0, 1.0),
            Side::Right => (room.px_w() - TILE, -1.0),
        };
        let y0 = e.rows.0 as f32 * TILE;
        let y1 = (e.rows.1 + 1) as f32 * TILE;
        draw_rectangle(x, y0 - 3.0, TILE, 3.0, GOLD);
        draw_triangle(vec2(x + 5.0, y0 - 3.0), vec2(x + 11.0, y0 - 3.0), vec2(x + 8.0, y0 + 2.0), GOLD_LIGHT);
        let edge = if e.side == Side::Left { 0.0 } else { room.px_w() };
        if room.sealed {
            for i in 0..4 {
                let bx = edge + inward * (2.0 + i as f32 * 4.0) - 1.0;
                draw_rectangle(bx, y0, 2.0, y1 - y0, IRON);
                draw_triangle(vec2(bx - 1.0, y0 + 3.0), vec2(bx + 3.0, y0 + 3.0), vec2(bx + 1.0, y0 - 1.0), GOLD_DARK);
            }
        } else {
            glow(glows, vec2(edge + inward * 4.0, (y0 + y1) / 2.0), 30.0, Color::new(1.0, 0.8, 0.55, 0.22));
        }
    }
}

fn interactables(g: &Game, glows: &mut Glows) {
    for it in &g.interactables {
        let (x, fy) = (it.pos.x.round(), it.pos.y);
        match it.kind {
            InterKind::Relic => {
                draw_rectangle(x - 7.0, fy - 3.0, 14.0, 3.0, MARBLE_TOP);
                draw_rectangle(x - 4.0, fy - 12.0, 8.0, 9.0, MARBLE_TOP_DARK);
                draw_rectangle(x - 2.0, fy - 12.0, 1.0, 9.0, with_alpha(CREAM, 0.5));
                draw_rectangle(x - 6.0, fy - 14.0, 12.0, 2.0, GOLD);
                if !g.flags.ribbon {
                    let y = fy - 21.0 + (g.time * 2.0).sin() * 1.5;
                    draw_line(x, y, x - 3.0, y + 7.0, 1.0, ROSE_DARK);
                    draw_line(x, y, x + 3.0, y + 7.0, 1.0, ROSE_DARK);
                    draw_triangle(vec2(x, y), vec2(x - 6.0, y - 3.0), vec2(x - 6.0, y + 3.0), ROSE);
                    draw_triangle(vec2(x, y), vec2(x + 6.0, y - 3.0), vec2(x + 6.0, y + 3.0), ROSE);
                    draw_circle(x, y, 1.5, CRIMSON);
                    let pulse = 0.4 + 0.15 * (g.time * 3.0).sin();
                    glow(glows, vec2(x, y), 30.0, Color::new(1.0, 0.5, 0.7, pulse));
                }
            }
            InterKind::Letter => {
                draw_rectangle(x - 9.0, fy - 9.0, 1.0, 9.0, WOOD_LIGHT);
                draw_rectangle(x + 8.0, fy - 9.0, 1.0, 9.0, WOOD_LIGHT);
                draw_rectangle(x - 11.0, fy - 11.0, 22.0, 2.0, WOOD_LIGHT);
                draw_rectangle(x - 11.0, fy - 9.0, 22.0, 1.0, GOLD_DARK);
                draw_rectangle(x - 4.0, fy - 15.0, 8.0, 4.0, CREAM);
                draw_line(x - 4.0, fy - 15.0, x, fy - 13.0, 1.0, MARBLE_TOP_DARK);
                draw_line(x, fy - 13.0, x + 4.0, fy - 15.0, 1.0, MARBLE_TOP_DARK);
                draw_circle(x, fy - 13.0, 1.2, CRIMSON);
                candle(x + 8.0, fy - 14.0, g.time, 4.2, glows);
                if !g.flags.letter_read {
                    let pulse = 0.25 + 0.1 * (g.time * 2.5).sin();
                    glow(glows, vec2(x, fy - 13.0), 18.0, Color::new(1.0, 0.85, 0.55, pulse));
                }
            }
        }
    }
}

/// Draws relative to a character's feet, mirrored by facing. Positive x is
/// "in front", negative y is up.
struct Pen {
    o: Vec2,
    f: f32,
    white: bool,
    alpha: f32,
}

impl Pen {
    fn new(feet: Vec2, facing: f32) -> Self {
        Self { o: feet.round(), f: facing, white: false, alpha: 1.0 }
    }

    fn c(&self, c: Color) -> Color {
        let c = if self.white { Color::new(1.0, 0.95, 0.95, c.a) } else { c };
        with_alpha(c, c.a * self.alpha)
    }

    fn p(&self, x: f32, y: f32) -> Vec2 {
        vec2(self.o.x + x * self.f, self.o.y + y)
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32, col: Color) {
        let x0 = if self.f > 0.0 { self.o.x + x } else { self.o.x - x - w };
        draw_rectangle(x0, self.o.y + y, w, h, self.c(col));
    }

    fn circle(&self, x: f32, y: f32, r: f32, col: Color) {
        let p = self.p(x, y);
        draw_circle(p.x, p.y, r, self.c(col));
    }

    fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32, th: f32, col: Color) {
        let (a, b) = (self.p(x1, y1), self.p(x2, y2));
        draw_line(a.x, a.y, b.x, b.y, th, self.c(col));
    }

    fn tri(&self, a: (f32, f32), b: (f32, f32), c: (f32, f32), col: Color) {
        draw_triangle(self.p(a.0, a.1), self.p(b.0, b.1), self.p(c.0, c.1), self.c(col));
    }

    fn quad(&self, a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32), col: Color) {
        self.tri(a, b, c, col);
        self.tri(a, c, d, col);
    }

    /// An open folding fan: alternating cream and rose ribs with a gold rim.
    fn fan(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
        let n = 7;
        let point = |i: i32| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            (cx + a.cos() * r, cy + a.sin() * r)
        };
        for i in 0..n {
            let col = if i % 2 == 0 { CREAM } else { ROSE };
            self.tri((cx, cy), point(i), point(i + 1), col);
            let (p, q) = (point(i), point(i + 1));
            self.line(p.0, p.1, q.0, q.1, 1.0, GOLD);
        }
    }
}

fn heroine(p: &Player, ribbon: bool, glows: &mut Glows) {
    if p.invuln > 0.0 && (p.invuln * 18.0) as i32 % 2 == 0 {
        return;
    }
    // A faint warm halo keeps her readable against dark rooms.
    glow(glows, p.center() - vec2(0.0, 2.0), 24.0, Color::new(1.0, 0.7, 0.75, 0.16));
    let pen = Pen::new(p.feet(), p.facing);
    let running = p.on_ground && p.vel.x.abs() > 10.0;
    let s = if running { (p.anim * 14.0).sin() } else { 0.0 };
    let flutter = if running {
        3.0 + s
    } else if !p.on_ground {
        2.0 - p.vel.y.signum() * 1.5
    } else {
        (p.anim * 2.0).sin() * 0.5
    };

    // Legs in white stockings and rose slippers.
    if p.on_ground {
        pen.rect(-2.0 - s * 2.5, -7.0, 2.0, 7.0, STOCKING_SHADE);
        pen.rect(-3.0 - s * 2.5, -1.0, 3.0, 1.0, ROSE_DARK);
        pen.rect(s * 2.5, -7.0, 2.0, 7.0, STOCKING);
        pen.rect(s * 2.5, -1.0, 3.0, 1.0, ROSE_DARK);
    } else {
        pen.rect(-3.0, -8.0, 2.0, 6.0, STOCKING_SHADE);
        pen.rect(1.0, -9.0, 2.0, 5.0, STOCKING);
        pen.rect(1.0, -5.0, 3.0, 1.0, ROSE_DARK);
    }
    // Coat tails streaming behind, then the coat itself.
    pen.tri((-2.0, -12.0), (-2.0, -6.0), (-7.0 - flutter, -4.0 + flutter * 0.4), ROSE_DARK);
    pen.quad((-3.0, -13.0), (3.0, -13.0), (4.0, -6.0), (-5.0, -6.0), ROSE);
    pen.rect(-4.0, -7.0, 8.0, 1.0, CREAM);
    pen.rect(-3.0, -17.0, 6.0, 5.0, ROSE);
    pen.rect(0.0, -17.0, 2.0, 5.0, CREAM);
    pen.rect(-3.0, -13.0, 6.0, 1.0, CRIMSON);
    pen.rect(-2.0, -18.0, 4.0, 1.0, CREAM);
    // Powdered hair with a rose, and her face.
    pen.circle(-1.0, -21.0, 3.5, HAIR);
    pen.circle(-3.5, -22.0, 2.2, HAIR_SHADE);
    pen.circle(0.8, -20.5, 2.6, SKIN);
    pen.rect(-2.0, -25.0, 5.0, 2.0, HAIR);
    pen.circle(-3.0, -24.0, 1.2, CRIMSON);
    pen.rect(2.0, -21.0, 1.0, 1.0, INK);
    if ribbon {
        pen.line(-4.0, -21.0, -7.0 - flutter, -19.0 + flutter * 0.3, 1.0, ROSE);
    }

    // The rapier.
    match p.attack_t {
        Some(t) => {
            let k = (t / ATTACK_TIME * 2.5).min(1.0);
            let hand = 4.0 + k * 3.0;
            pen.line(1.0, -15.0, hand, -15.0, 1.0, ROSE);
            pen.line(hand, -15.0, hand + 9.0 + k * 13.0, -15.0, 1.0, SILVER);
            pen.circle(hand, -15.0, 1.2, GOLD);
            if (0.03..=0.15).contains(&t) {
                pen.line(hand + 4.0, -14.0, hand + 22.0, -14.0, 1.0, with_alpha(WHITE, 0.6));
                glow(glows, pen.p(hand + 20.0, -15.0), 10.0, Color::new(1.0, 0.95, 0.85, 0.5));
            }
        }
        None if p.cast_t > 0.0 => {
            pen.line(1.0, -16.0, 5.0, -18.0, 1.0, ROSE);
            pen.line(3.0, -12.0, 13.0, -5.0, 1.0, SILVER);
            glow(glows, pen.p(5.0, -18.0), 12.0, Color::new(1.0, 0.5, 0.7, 0.6));
        }
        None => {
            pen.line(1.0, -16.0, 3.0, -12.0, 1.0, ROSE);
            pen.line(3.0, -12.0, 13.0, -5.0, 1.0, SILVER);
            pen.circle(3.0, -12.0, 1.2, GOLD);
        }
    }
    if p.heal_t > 0.0 {
        glow(glows, p.center(), 26.0, Color::new(1.0, 0.85, 0.6, p.heal_t * 0.7));
    }
}

fn enemy(e: &Enemy, time: f32, glows: &mut Glows) {
    let mut pen = Pen::new(e.feet(), e.facing);
    pen.white = e.flash > 0.0;
    match e.kind {
        EnemyKind::Halberdier => halberdier(e, &pen, glows),
        EnemyKind::Moth => {
            let pen = Pen::new(e.center(), e.facing);
            let flap = (e.anim * 18.0).sin();
            let wing = if e.flash > 0.0 { WHITE } else { with_alpha(MOTH, 0.9) };
            pen.tri((0.0, 0.0), (-7.0, -4.0 - flap * 3.0), (-2.0, 2.0), wing);
            pen.tri((0.0, 0.0), (7.0, -4.0 - flap * 3.0), (2.0, 2.0), wing);
            pen.tri((0.0, 1.0), (-4.0, 4.0 + flap), (0.0, 3.0), with_alpha(MOTH, 0.6));
            pen.tri((0.0, 1.0), (4.0, 4.0 + flap), (0.0, 3.0), with_alpha(MOTH, 0.6));
            pen.rect(-1.0, -2.0, 2.0, 5.0, WOOD_DARK);
            pen.rect(1.0, -2.0, 1.0, 1.0, CRIMSON_LIGHT);
            glow(glows, e.center(), 14.0, Color::new(0.7, 0.6, 1.0, 0.25 + 0.1 * (time * 4.0).sin()));
        }
        EnemyKind::Marquise => marquise(e, &mut pen, glows),
    }
}

fn halberdier(e: &Enemy, pen: &Pen, glows: &mut Glows) {
    let walking = e.ai == Ai::Patrol && e.vel.x.abs() > 1.0;
    let s = if walking { (e.anim * 8.0).sin() } else { 0.0 };
    pen.rect(-5.0 - s * 1.5, -10.0, 4.0, 10.0, ARMOR_DARK);
    pen.rect(1.0 + s * 1.5, -10.0, 4.0, 10.0, ARMOR);
    pen.rect(-6.0 - s * 1.5, -2.0, 5.0, 2.0, INK);
    pen.rect(1.0 + s * 1.5, -2.0, 5.0, 2.0, INK);
    pen.rect(-6.0, -14.0, 12.0, 5.0, ARMOR_DARK);
    pen.rect(-6.0, -24.0, 12.0, 11.0, ARMOR);
    pen.rect(-4.0, -23.0, 2.0, 8.0, GOLD_LIGHT);
    pen.line(-6.0, -23.0, 6.0, -14.0, 2.0, CRIMSON);
    pen.circle(-6.0, -23.0, 3.0, ARMOR_DARK);
    pen.circle(6.0, -23.0, 3.0, ARMOR);
    pen.rect(-2.0, -29.0, 5.0, 5.0, SKIN);
    pen.rect(0.0, -26.0, 4.0, 1.0, INK);
    pen.rect(-7.0, -31.0, 15.0, 2.0, INK);
    pen.tri((-6.0, -30.0), (7.0, -30.0), (0.0, -35.0), INK);
    pen.circle(-5.0, -34.0, 1.6, CRIMSON);
    pen.circle(-7.0, -33.0, 1.6, CRIMSON);
    pen.circle(-8.5, -31.0, 1.4, CRIMSON_LIGHT);

    match e.ai {
        Ai::Windup => {
            pen.line(-14.0, -17.0, 10.0, -20.0, 2.0, WOOD_LIGHT);
            pen.tri((-14.0, -17.0), (-19.0, -21.0), (-18.0, -14.0), SILVER);
            if e.t > HALBERD_WINDUP * 0.5 {
                let tip = pen.p(-18.0, -18.0);
                glow(glows, tip, 10.0, Color::new(1.0, 1.0, 1.0, 0.8));
            }
        }
        Ai::Strike => {
            let reach = (e.t / 0.08).min(1.0) * 40.0;
            pen.line(-4.0, -18.0, reach + 4.0, -18.0, 2.0, WOOD_LIGHT);
            pen.tri((reach + 4.0, -21.0), (reach + 12.0, -18.0), (reach + 4.0, -15.0), SILVER);
            pen.tri((reach, -18.0), (reach + 3.0, -24.0), (reach + 6.0, -18.0), SILVER);
        }
        _ => {
            pen.line(8.0, -40.0, 8.0, 0.0, 2.0, WOOD_LIGHT);
            pen.tri((8.0, -38.0), (14.0, -35.0), (8.0, -31.0), SILVER);
            pen.line(8.0, -38.0, 8.0, -43.0, 1.0, SILVER);
            pen.circle(8.0, -21.0, 1.5, ARMOR);
        }
    }
}

fn marquise(e: &Enemy, pen: &mut Pen, glows: &mut Glows) {
    if e.ai == Ai::Dying {
        pen.alpha = (1.0 - e.t / MARQUISE_DEATH_TIME).max(0.0);
    }
    let sway = (e.anim * 2.0).sin();
    let telegraph = matches!(e.ai, Ai::FanWindup | Ai::LungeWindup | Ai::VolleyWindup);

    // The gown: a great bell of silk with ruffled hem and bows.
    pen.quad((-7.0, -30.0), (7.0, -30.0), (22.0 + sway, 0.0), (-22.0 + sway, 0.0), GOWN);
    pen.quad((-7.0, -30.0), (-2.0, -30.0), (-8.0 + sway, 0.0), (-22.0 + sway, 0.0), GOWN_SHADE);
    for i in -2..=2 {
        let x = i as f32 * 5.0;
        pen.line(x * 0.3, -29.0, x * 2.6 + sway, -1.0, 1.0, with_alpha(GOLD, 0.7));
    }
    for i in 0..9 {
        let x = -20.0 + i as f32 * 5.0 + sway;
        pen.circle(x, -1.5, 2.2, if i % 2 == 0 { ROSE } else { CREAM });
    }
    pen.tri((-9.0, -16.0), (-5.0, -18.0), (-5.0, -14.0), ROSE);
    pen.tri((7.0, -10.0), (11.0, -12.0), (11.0, -8.0), ROSE);
    // Bodice, lace stomacher and neckline.
    pen.rect(-5.0, -37.0, 10.0, 8.0, GOWN_SHADE);
    pen.tri((-3.0, -37.0), (3.0, -37.0), (0.0, -29.0), CREAM);
    pen.rect(-4.0, -39.0, 8.0, 2.0, SKIN);
    // Masked face beneath a towering powdered wig.
    for (x, y, r) in [
        (0.0, -48.0, 6.0),
        (-3.0, -53.0, 5.0),
        (2.0, -57.0, 5.0),
        (0.0, -62.0, 4.0),
        (-6.0, -46.0, 3.0),
        (6.0, -46.0, 3.0),
    ] {
        pen.circle(x, y, r, HAIR);
    }
    pen.circle(-4.0, -55.0, 1.6, ROSE);
    pen.circle(3.0, -60.0, 1.4, CRIMSON);
    pen.line(-1.0, -63.0, -6.0, -70.0, 1.0, CREAM);
    pen.circle(0.5, -43.0, 4.0, MASK);
    let eyes = if telegraph { CRIMSON_LIGHT } else { INK };
    pen.rect(2.0, -44.0, 2.0, 1.0, eyes);
    pen.rect(-2.0, -44.0, 2.0, 1.0, eyes);
    pen.rect(0.0, -40.0, 2.0, 1.0, CRIMSON);
    if telegraph {
        glow(glows, pen.p(1.0, -44.0), 10.0, Color::new(1.0, 0.2, 0.3, 0.6));
    }

    // Arms, fan and rapier depending on what she's doing.
    match e.ai {
        Ai::FanWindup => {
            pen.line(-4.0, -35.0, -12.0, -46.0, 1.0, SKIN);
            pen.fan(-12.0, -46.0, 9.0, -2.6, -0.6);
        }
        Ai::FanOut => {
            pen.line(4.0, -34.0, 14.0, -32.0, 1.0, SKIN);
        }
        Ai::LungeWindup => {
            pen.line(-4.0, -34.0, -10.0, -32.0, 1.0, SKIN);
            pen.line(-10.0, -32.0, -28.0, -36.0, 1.0, SILVER);
            pen.circle(-10.0, -32.0, 1.5, GOLD);
            if e.t > 0.3 {
                glow(glows, pen.p(-28.0, -36.0), 9.0, Color::new(1.0, 1.0, 1.0, 0.9));
            }
            pen.fan(5.0, -34.0, 7.0, -2.2, -0.4);
        }
        Ai::Lunge => {
            pen.line(4.0, -32.0, 12.0, -32.0, 1.0, SKIN);
            pen.line(12.0, -32.0, 36.0, -31.0, 1.0, SILVER);
            pen.circle(12.0, -32.0, 1.5, GOLD);
            pen.line(-10.0, -20.0, -30.0, -20.0, 1.0, with_alpha(CREAM, 0.5));
            pen.line(-12.0, -8.0, -34.0, -8.0, 1.0, with_alpha(CREAM, 0.4));
        }
        Ai::VolleyWindup => {
            pen.line(0.0, -37.0, 0.0, -66.0, 1.0, SKIN);
            pen.fan(0.0, -66.0, 10.0, -3.0, -0.1);
            glow(glows, pen.p(0.0, -70.0), 20.0, Color::new(1.0, 0.2, 0.35, 0.5));
        }
        _ => {
            pen.line(4.0, -34.0, 9.0, -28.0, 1.0, SKIN);
            pen.fan(9.0, -28.0, 7.0, -1.9 + sway * 0.15, -0.9 + sway * 0.15);
        }
    }
}

fn projectiles(g: &Game, glows: &mut Glows) {
    for p in &g.projectiles {
        match p.kind {
            ProjKind::Petal => {
                let back = p.pos - p.vel.normalize_or_zero() * 8.0;
                draw_line(back.x, back.y, p.pos.x, p.pos.y, 1.0, with_alpha(ROSE, 0.4));
                draw_poly(p.pos.x, p.pos.y, 4, 3.0, p.t * 900.0, ROSE);
                glow(glows, p.pos, 12.0, Color::new(1.0, 0.45, 0.65, 0.6));
            }
            ProjKind::Fan => {
                let pen = Pen::new(p.pos, 1.0);
                let spin = p.t * 14.0;
                pen.fan(0.0, 0.0, 7.0, spin, spin + 3.6);
                glow(glows, p.pos, 16.0, Color::new(1.0, 0.8, 0.85, 0.4));
            }
            ProjKind::Thorn => {
                let rot = p.vel.y.atan2(p.vel.x).to_degrees();
                draw_poly(p.pos.x, p.pos.y, 4, 3.0, rot + 45.0, CRIMSON_LIGHT);
                glow(glows, p.pos, 10.0, Color::new(1.0, 0.15, 0.25, 0.7));
            }
        }
    }
}

fn particles(g: &Game) {
    for p in &g.particles {
        let k = (p.life / p.max_life).clamp(0.0, 1.0);
        let c = with_alpha(p.color, p.color.a * k.min(1.0).sqrt());
        match p.kind {
            PKind::Spark => draw_rectangle(p.pos.x, p.pos.y, p.size, p.size, c),
            PKind::Petal => {
                let rot = p.seed * 40.0 + (p.max_life - p.life) * 200.0;
                draw_poly(p.pos.x, p.pos.y, 4, p.size + 0.5, rot, c);
            }
            PKind::Mote => {
                let twinkle = 0.5 + 0.5 * (g.time * 2.0 + p.seed).sin();
                draw_rectangle(p.pos.x, p.pos.y, 1.0, 1.0, with_alpha(c, c.a * twinkle * (1.0 - k).min(k) * 2.0));
            }
            PKind::Puff => draw_circle(p.pos.x, p.pos.y, p.size + (1.0 - k) * 3.0, with_alpha(c, c.a * k)),
        }
    }
}

fn hud(g: &Game) {
    let p = &g.player;
    // Portrait medallion.
    let (mx, my) = (22.0, 22.0);
    draw_circle(mx, my, 16.0, GOLD_DEEP);
    draw_circle(mx, my, 15.0, GOLD);
    draw_circle(mx, my, 13.0, NAVY);
    draw_rectangle(mx - 6.0, my + 6.0, 12.0, 5.0, ROSE);
    draw_rectangle(mx - 2.0, my + 6.0, 4.0, 2.0, CREAM);
    draw_circle(mx - 1.0, my - 2.0, 7.0, HAIR);
    draw_circle(mx - 5.0, my - 4.0, 3.5, HAIR_SHADE);
    draw_circle(mx + 1.5, my + 0.5, 4.5, SKIN);
    draw_rectangle(mx + 3.0, my - 1.0, 1.0, 1.0, INK);
    draw_circle(mx - 5.0, my - 7.0, 1.6, CRIMSON);
    draw_circle_lines(mx, my, 15.0, 1.0, GOLD_LIGHT);
    draw_poly(mx, my - 16.0, 4, 3.0, 0.0, GOLD_LIGHT);
    draw_poly(mx, my + 16.0, 4, 2.0, 0.0, GOLD);

    bar(42.0, 13.0, 100.0, 7.0, g.hp_ghost / player::MAX_HP, p.hp / player::MAX_HP, CRIMSON, CRIMSON_LIGHT);
    bar(42.0, 24.0, 74.0, 4.0, 0.0, p.mana / player::MAX_MANA, MANA, MANA_LIGHT);
    for i in 0..player::MAX_FLASKS {
        fleur(48.0 + i as f32 * 11.0, 37.0, i < p.flasks);
    }
    if g.flags.ribbon {
        let (x, y) = (90.0, 36.0);
        draw_triangle(vec2(x, y), vec2(x - 5.0, y - 3.0), vec2(x - 5.0, y + 3.0), ROSE);
        draw_triangle(vec2(x, y), vec2(x + 5.0, y - 3.0), vec2(x + 5.0, y + 3.0), ROSE);
        draw_circle(x, y, 1.3, CRIMSON);
    }

    if let (Some(boss), Fight::Intro(_) | Fight::Active) = (g.boss(), g.fight) {
        let fill = (boss.hp.max(0) as f32 / boss.max_hp as f32).clamp(0.0, 1.0);
        bar(110.0, 254.0, 260.0, 5.0, fill, fill, CRIMSON, ROSE);
    }
}

fn bar(x: f32, y: f32, w: f32, h: f32, ghost: f32, value: f32, fill: Color, shine: Color) {
    draw_rectangle(x - 1.0, y - 1.0, w + 2.0, h + 2.0, GOLD_DEEP);
    draw_rectangle(x, y, w, h, INK);
    draw_rectangle(x, y, w * ghost.clamp(0.0, 1.0), h, Color::new(0.95, 0.82, 0.82, 0.6));
    draw_rectangle(x, y, w * value.clamp(0.0, 1.0), h, fill);
    draw_rectangle(x, y, w * value.clamp(0.0, 1.0), 1.0, shine);
    draw_rectangle_lines(x - 1.0, y - 1.0, w + 2.0, h + 2.0, 1.0, GOLD);
    draw_poly(x + w + 3.0, y + h / 2.0, 4, 2.5, 0.0, GOLD);
}

/// A fleur-de-lis pip for each rosewater flask.
fn fleur(x: f32, y: f32, full: bool) {
    let c = if full { ROSE } else { with_alpha(HAIR_SHADE, 0.3) };
    let band = if full { GOLD } else { with_alpha(GOLD_DARK, 0.5) };
    draw_triangle(vec2(x, y - 6.0), vec2(x - 2.0, y), vec2(x + 2.0, y), c);
    draw_circle(x - 3.0, y - 2.0, 1.6, c);
    draw_circle(x + 3.0, y - 2.0, 1.6, c);
    draw_rectangle(x - 3.0, y, 6.0, 1.0, band);
    draw_triangle(vec2(x, y + 1.0), vec2(x - 2.0, y + 4.0), vec2(x + 2.0, y + 4.0), c);
}

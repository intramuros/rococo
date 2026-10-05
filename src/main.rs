//! Éclat — a small rococo metroidvania fragment.
//!
//! Run with `cargo run --release`. For a headless screenshot of a named
//! moment (title, gallery, fight, library, letter, relic, ledge, boss,
//! victory, dead): `cargo run --release -- --shot boss out.png`.

mod draw;
mod enemy;
mod game;
mod input;
mod palette;
mod player;
mod ui;
mod world;

use macroquad::prelude::*;

use game::Game;
use input::Input;

/// The game renders at this resolution and is upscaled with crisp pixels.
pub const VIEW_W: f32 = 480.0;
pub const VIEW_H: f32 = 270.0;
const STEP: f32 = 1.0 / 120.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "Éclat".to_owned(),
        window_width: 1440,
        window_height: 810,
        high_dpi: false,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let shot = args.iter().position(|a| a == "--shot").map(|i| {
        let arg = |n: usize| args.get(i + n).expect("usage: --shot <scene> <out.png>").clone();
        (arg(1), arg(2))
    });

    let assets = draw::Assets::load();
    let view = render_target(VIEW_W as u32, VIEW_H as u32);
    view.texture.set_filter(FilterMode::Nearest);

    let mut game = Game::new();
    if let Some((scene, path)) = &shot {
        for _ in 0..game.debug_scene(scene) {
            game.update(STEP, Input::default());
        }
        render(&game, &assets, &view);
        let mut frame = get_screen_data();
        frame.bytes.chunks_exact_mut(4).for_each(|px| px[3] = 255);
        frame.export_png(path);
        return;
    }

    let mut acc = 0.0;
    let mut pending = Input::default();
    let mut fullscreen = false;
    loop {
        if is_key_pressed(KeyCode::F11) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
        }
        pending = pending.merge(Input::poll());
        acc += get_frame_time().min(0.1);
        while acc >= STEP {
            game.update(STEP, pending);
            pending = pending.held_only();
            acc -= STEP;
        }
        render(&game, &assets, &view);
        next_frame().await;
    }
}

fn render(game: &Game, assets: &draw::Assets, view: &RenderTarget) {
    draw::world(game, assets, view);
    set_default_camera();
    clear_background(BLACK);
    let vp = ui::Viewport::fit();
    let params = DrawTextureParams { dest_size: Some(vec2(VIEW_W, VIEW_H) * vp.s), flip_y: true, ..Default::default() };
    gl_use_material(&assets.opaque);
    draw_texture_ex(&view.texture, vp.ox, vp.oy, WHITE, params);
    gl_use_default_material();
    ui::draw(game, assets, &vp);
}

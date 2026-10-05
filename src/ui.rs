//! Text and overlays, drawn at full window resolution on top of the
//! upscaled pixel view so lettering stays crisp.

use macroquad::prelude::*;

use crate::draw::{self, Assets};
use crate::game::{Fight, Game, Popup, Scene};
use crate::palette::GOLD;
use crate::palette::*;
use crate::{VIEW_H, VIEW_W};

/// Where the 480×270 view sits in the window.
pub struct Viewport {
    pub ox: f32,
    pub oy: f32,
    pub s: f32,
}

impl Viewport {
    pub fn fit() -> Self {
        let s = (screen_width() / VIEW_W).min(screen_height() / VIEW_H);
        let s = if s >= 1.0 { s.floor() } else { s };
        Self {
            ox: ((screen_width() - VIEW_W * s) / 2.0).floor(),
            oy: ((screen_height() - VIEW_H * s) / 2.0).floor(),
            s,
        }
    }

    /// View coordinates (0..480, 0..270) to window pixels.
    fn at(&self, x: f32, y: f32) -> Vec2 {
        vec2(self.ox + x * self.s, self.oy + y * self.s)
    }
}

struct Ink<'a> {
    vp: &'a Viewport,
}

impl Ink<'_> {
    /// Draws `text` centred on view-x `cx` with its baseline at view-y `y`.
    /// `size` and `tracking` are in view pixels.
    fn centered(&self, font: &Font, text: &str, cx: f32, y: f32, size: f32, tracking: f32, color: Color) {
        let px = (size * self.vp.s).round() as u16;
        let gap = tracking * self.vp.s;
        let widths: Vec<f32> =
            text.chars().map(|c| measure_text(c.encode_utf8(&mut [0; 4]), Some(font), px, 1.0).width).collect();
        let total = if tracking == 0.0 {
            measure_text(text, Some(font), px, 1.0).width
        } else {
            widths.iter().sum::<f32>() + gap * (widths.len().saturating_sub(1)) as f32
        };
        let origin = self.vp.at(cx, y);
        let shadow = with_alpha(BLACK, 0.65 * color.a);
        let mut x = origin.x - total / 2.0;
        let params = |c| TextParams { font: Some(font), font_size: px, color: c, ..Default::default() };
        if tracking == 0.0 {
            draw_text_ex(text, x + self.vp.s * 0.6, origin.y + self.vp.s * 0.6, params(shadow));
            draw_text_ex(text, x, origin.y, params(color));
            return;
        }
        for (ch, w) in text.chars().zip(widths) {
            let s = ch.to_string();
            draw_text_ex(&s, x + self.vp.s * 0.6, origin.y + self.vp.s * 0.6, params(shadow));
            draw_text_ex(&s, x, origin.y, params(color));
            x += w + gap;
        }
    }

    /// A gold rule with a diamond in the middle, like the key art's flourishes.
    fn flourish(&self, cx: f32, y: f32, half: f32, alpha: f32) {
        let a = self.vp.at(cx - half, y);
        let b = self.vp.at(cx + half, y);
        let c = self.vp.at(cx, y);
        let gold = with_alpha(GOLD, alpha);
        draw_line(a.x, a.y, c.x - 6.0 * self.vp.s, c.y, self.vp.s * 0.5, gold);
        draw_line(c.x + 6.0 * self.vp.s, c.y, b.x, b.y, self.vp.s * 0.5, gold);
        draw_poly(c.x, c.y, 4, 3.0 * self.vp.s, 0.0, gold);
        draw_poly(c.x, c.y, 4, 1.5 * self.vp.s, 0.0, with_alpha(INK, alpha));
    }

    fn diamond(&self, x: f32, y: f32, r: f32, color: Color) {
        let c = self.vp.at(x, y);
        draw_poly(c.x, c.y, 4, r * self.vp.s, 0.0, color);
    }

    fn wrap(&self, font: &Font, text: &str, size: f32, max_w: f32) -> Vec<String> {
        let px = (size * self.vp.s).round() as u16;
        let max_w = max_w * self.vp.s;
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split_whitespace() {
                let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if !line.is_empty() && measure_text(&candidate, Some(font), px, 1.0).width > max_w {
                    lines.push(std::mem::replace(&mut line, word.to_string()));
                } else {
                    line = candidate;
                }
            }
            lines.push(line);
        }
        lines
    }

    fn left(&self, font: &Font, text: &str, x: f32, y: f32, size: f32, color: Color) {
        let p = self.vp.at(x, y);
        let px = (size * self.vp.s).round() as u16;
        draw_text_ex(text, p.x, p.y, TextParams { font: Some(font), font_size: px, color, ..Default::default() });
    }

    fn veil(&self, color: Color) {
        let p = self.vp.at(0.0, 0.0);
        draw_rectangle(p.x, p.y, VIEW_W * self.vp.s, VIEW_H * self.vp.s, color);
    }
}

/// Fades in over `rise` seconds, holds, and fades out over the last `fall`.
fn envelope(t: f32, total: f32, rise: f32, fall: f32) -> f32 {
    (t / rise).min((total - t) / fall).clamp(0.0, 1.0)
}

pub fn draw(g: &Game, a: &Assets, vp: &Viewport) {
    let ink = Ink { vp };
    match g.scene {
        Scene::Title => title(g, a, &ink),
        Scene::Playing => playing(g, a, &ink),
        Scene::Popup(popup) => {
            playing(g, a, &ink);
            panel(a, &ink, popup);
        }
        Scene::Dead(t) => {
            ink.veil(Color::new(0.06, 0.0, 0.03, (t / 1.2).min(0.85)));
            let alpha = ((t - 0.4) / 0.8).clamp(0.0, 1.0);
            ink.centered(
                &a.display,
                "THE CANDLES GUTTER",
                VIEW_W / 2.0,
                128.0,
                20.0,
                2.0,
                with_alpha(CRIMSON_LIGHT, alpha),
            );
            ink.flourish(VIEW_W / 2.0, 142.0, 80.0, alpha);
            if t > 1.2 {
                ink.centered(
                    &a.body,
                    "Press Enter to rise again",
                    VIEW_W / 2.0,
                    162.0,
                    9.0,
                    0.0,
                    with_alpha(CREAM, 0.8),
                );
            }
        }
        Scene::Victory(t) => victory(g, a, &ink, t),
    }
    let fade = g.fade();
    if fade > 0.0 {
        ink.veil(with_alpha(BLACK, fade));
    }
}

fn title(g: &Game, a: &Assets, ink: &Ink) {
    let cx = VIEW_W / 2.0;
    let rise = (g.time / 1.5).min(1.0);
    ink.flourish(cx, 52.0, 60.0, rise);
    ink.centered(&a.display, "ÉCLAT", cx, 104.0, 52.0, 3.0, with_alpha(GOLD_LIGHT, rise));
    ink.flourish(cx, 118.0, 110.0, rise);
    ink.centered(&a.display, "A METROIDVANIA OF SHADOWS AND SILK", cx, 136.0, 9.5, 1.5, with_alpha(CREAM, rise));
    for (dx, word) in [(-78.0, "EXPLORE"), (0.0, "UNLOCK"), (78.0, "UNCOVER")] {
        ink.centered(&a.display, word, cx + dx, 158.0, 7.5, 1.5, with_alpha(ROSE, rise * 0.9));
    }
    for dx in [-39.0, 39.0] {
        ink.diamond(cx + dx, 155.5, 1.6, with_alpha(GOLD, rise));
    }
    if g.time > 1.2 {
        let blink = 0.55 + 0.45 * (g.time * 3.0).sin();
        ink.centered(&a.display, "PRESS ENTER", cx, 196.0, 9.0, 2.0, with_alpha(GOLD, blink));
    }
    let controls = "A D  move     Space  jump     J  lunge     K  petal dart     L  rosewater     E  examine";
    ink.centered(&a.body, controls, cx, 252.0, 7.5, 0.0, with_alpha(CREAM, 0.7 * rise));
}

fn playing(g: &Game, a: &Assets, ink: &Ink) {
    let cam = draw::camera(g);
    if let Some(i) = g.prompt {
        let p = g.interactables[i].pos - cam - vec2(0.0, 32.0);
        let c = ink.vp.at(p.x - 14.0, p.y);
        draw_circle(c.x, c.y, 5.5 * ink.vp.s, with_alpha(INK, 0.7));
        draw_circle_lines(c.x, c.y, 5.5 * ink.vp.s, 0.6 * ink.vp.s, GOLD);
        ink.centered(&a.display, "E", p.x - 14.0, p.y + 2.7, 7.0, 0.0, GOLD_LIGHT);
        ink.left(&a.display, "Examine", p.x - 6.0, p.y + 2.7, 7.5, CREAM);
    }

    if let Some((name, t)) = g.banner {
        let alpha = envelope(t, 3.2, 0.5, 0.8);
        ink.flourish(VIEW_W / 2.0, 58.0, 70.0, alpha);
        ink.centered(&a.display, &name.to_uppercase(), VIEW_W / 2.0, 76.0, 13.0, 2.0, with_alpha(GOLD_LIGHT, alpha));
        ink.flourish(VIEW_W / 2.0, 86.0, 70.0, alpha);
    }

    if g.room.id == crate::world::RoomId::Gallery && g.play_time < 12.0 && g.banner.is_none() {
        let alpha = envelope(g.play_time, 12.0, 1.0, 1.5) * 0.8;
        let hint = "A D move · Space jump (↓ + Space drops) · J lunge · K petal dart · L rosewater · E examine";
        ink.centered(&a.body, hint, VIEW_W / 2.0, 262.0, 7.5, 0.0, with_alpha(CREAM, alpha));
    }

    match (g.fight, g.boss()) {
        (Fight::Intro(t), Some(_)) => {
            let alpha = envelope(t, 2.4, 0.4, 0.6);
            ink.centered(
                &a.display,
                "LA MARQUISE MASQUÉE",
                VIEW_W / 2.0,
                96.0,
                18.0,
                2.5,
                with_alpha(GOLD_LIGHT, alpha),
            );
            ink.flourish(VIEW_W / 2.0, 108.0, 100.0, alpha);
            ink.centered(
                &a.body,
                "Hostess of the Endless Masque",
                VIEW_W / 2.0,
                124.0,
                10.0,
                0.0,
                with_alpha(ROSE, alpha),
            );
            ink.centered(
                &a.display,
                "LA MARQUISE MASQUÉE",
                VIEW_W / 2.0,
                249.0,
                7.0,
                1.5,
                with_alpha(CREAM, 1.0 - alpha),
            );
        }
        (Fight::Active, Some(_)) => {
            ink.centered(&a.display, "LA MARQUISE MASQUÉE", VIEW_W / 2.0, 249.0, 7.0, 1.5, CREAM);
        }
        _ => {}
    }
}

fn panel(a: &Assets, ink: &Ink, popup: Popup) {
    let (title, body) = match popup {
        Popup::Relic => (
            "SILK RIBBON OF THE COMTESSE",
            "Cut from the hem of a gown that danced for nine nights without rest. The silk still remembers how to rise.\n\nPress JUMP again while in the air to leap once more.",
        ),
        Popup::Letter => (
            "A LETTER, NEVER SENT",
            "My dearest Éloise,\n\nThe masque has not ended. Three winters now, and still the guests dance, though none of them have faces beneath their masks any longer. Mother says the house is only tired.\n\nIf you come home, do not trust the Marquise's smile. Watch her fan instead. It always turns before she does.\n\nYour sister, Adélaïde",
        ),
    };
    let size = 8.6;
    let w = 330.0;
    let lines = ink.wrap(&a.body, body, size, w - 44.0);
    let line_h = |line: &String| if line.is_empty() { size * 0.6 } else { size * 1.22 };
    let h = 74.0 + lines.iter().map(line_h).sum::<f32>();
    let (x, y) = ((VIEW_W - w) / 2.0, (VIEW_H - h) / 2.0);

    ink.veil(with_alpha(INK, 0.55));
    let s = ink.vp.s;
    let p = ink.vp.at(x, y);
    draw_rectangle(p.x, p.y, w * s, h * s, PARCHMENT);
    draw_rectangle_lines(p.x, p.y, w * s, h * s, 2.0 * s, GOLD_DARK);
    draw_rectangle_lines(p.x + 4.0 * s, p.y + 4.0 * s, (w - 8.0) * s, (h - 8.0) * s, 0.6 * s, GOLD);
    for (cx, cy) in [(x + 4.0, y + 4.0), (x + w - 4.0, y + 4.0), (x + 4.0, y + h - 4.0), (x + w - 4.0, y + h - 4.0)] {
        ink.diamond(cx, cy, 3.0, GOLD_DARK);
    }

    ink.centered(&a.display, title, VIEW_W / 2.0, y + 26.0, 11.0, 1.2, CRIMSON);
    ink.flourish(VIEW_W / 2.0, y + 35.0, 60.0, 1.0);
    let mut line_y = y + 52.0;
    for line in &lines {
        ink.left(&a.body, line, x + 22.0, line_y, size, LETTER_INK);
        line_y += line_h(line);
    }
    ink.centered(&a.display, "PRESS E TO CONTINUE", VIEW_W / 2.0, y + h - 12.0, 6.5, 1.2, GOLD_DARK);
}

fn victory(g: &Game, a: &Assets, ink: &Ink, t: f32) {
    ink.veil(with_alpha(INK, (t / 2.0).min(0.8)));
    let alpha = |start: f32| ((t - start) / 1.0).clamp(0.0, 1.0);
    let cx = VIEW_W / 2.0;
    ink.centered(&a.display, "BEAUTY HIDES", cx, 82.0, 20.0, 3.0, with_alpha(GOLD_LIGHT, alpha(0.5)));
    ink.centered(&a.display, "ANOTHER WORLD", cx, 108.0, 20.0, 3.0, with_alpha(GOLD_LIGHT, alpha(1.0)));
    ink.flourish(cx, 122.0, 110.0, alpha(1.0));
    let epilogue = "The Marquise's mask cracks. Behind it there is only a cold draught and the smell of roses.";
    ink.centered(&a.body, epilogue, cx, 142.0, 9.0, 0.0, with_alpha(CREAM, alpha(1.8)));
    let minutes = (g.play_time / 60.0) as u32;
    let seconds = (g.play_time % 60.0) as u32;
    let stats = format!("Time {minutes}:{seconds:02}     Falls {}", g.deaths);
    ink.centered(&a.display, &stats, cx, 170.0, 8.0, 1.2, with_alpha(ROSE, alpha(2.4)));
    ink.centered(
        &a.body,
        "Thank you for playing this fragment of Éclat.",
        cx,
        200.0,
        9.0,
        0.0,
        with_alpha(CREAM, alpha(3.0)),
    );
    if t > 3.0 {
        let blink = 0.55 + 0.45 * (g.time * 3.0).sin();
        ink.centered(&a.display, "PRESS ENTER", cx, 228.0, 8.0, 2.0, with_alpha(GOLD, blink));
    }
}

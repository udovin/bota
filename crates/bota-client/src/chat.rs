//! The chat as it is drawn: in a box on the hall's screens and in the lobby,
//! over the field in a match.

use bota_proto::Team;
use macroquad::prelude::*;

use crate::hud::UiRect;
use crate::state::{App, ChatLine, Phase};

/// Seconds a line stays over the field before it is gone.
pub const FIELD_LIFE: f32 = 12.0;
/// Most lines shown over the field at once.
const FIELD_LINES: usize = 8;
/// Size of the chat's letters.
const SIZE: f32 = 18.0;
/// Height of one shown line.
const LINE_H: f32 = 22.0;

/// The text of a line as it is shown.
pub fn shown(line: &ChatLine) -> String {
    match &line.from {
        None => line.text.clone(),
        Some((from, _)) if line.team => format!("[team] {from}: {}", line.text),
        Some((from, _)) => format!("{from}: {}", line.text),
    }
}

/// The colour a line is drawn in: the speaker's side, plain for one with no
/// side, gold for the server's own.
fn tint(line: &ChatLine, alpha: f32) -> Color {
    let base = match &line.from {
        None => Color::new(0.95, 0.80, 0.40, 1.0),
        Some((_, Some(Team::Radiant))) => Color::new(0.55, 0.90, 0.55, 1.0),
        Some((_, Some(Team::Dire))) => Color::new(0.95, 0.55, 0.50, 1.0),
        Some((_, _)) => Color::new(0.88, 0.90, 0.94, 1.0),
    };
    Color { a: alpha, ..base }
}

/// A text cut at spaces into pieces no wider than a width. A word wider than
/// that is a piece of its own.
fn wrap(text: &str, width: f32) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        let longer = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if current.is_empty() || measure_text(&longer, None, SIZE as u16, 1.0).width <= width {
            current = longer;
        } else {
            pieces.push(std::mem::replace(&mut current, word.to_string()));
        }
    }
    pieces.push(current);
    pieces
}

/// The end of a text that fits a width, marked where it was cut.
fn tail(text: &str, width: f32) -> String {
    let mut kept: Vec<char> = text.chars().collect();
    let mut cut = false;
    while !kept.is_empty() {
        let shown: String = kept.iter().collect();
        let shown = if cut { format!("..{shown}") } else { shown };
        if measure_text(&shown, None, SIZE as u16, 1.0).width <= width {
            return shown;
        }
        kept.remove(0);
        cut = true;
    }
    String::new()
}

/// The line being typed, as it is shown: where it goes, then what it says.
fn typing(app: &App) -> Option<String> {
    let draft = app.draft.as_ref()?;
    let to = match app.phase {
        Phase::Hall | Phase::Cup => "hall",
        _ if draft.team => "team",
        _ => "all",
    };
    Some(format!("{to}> {}_", draft.text))
}

/// Draws the chat in a box: the newest lines at the bottom, and under them
/// the line being typed, or a hint at how to start one.
pub fn draw_box(app: &App, rect: &UiRect, hint: &str) {
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        Color::new(0.10, 0.12, 0.15, 0.92),
    );
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        1.0,
        Color::new(0.35, 0.40, 0.50, 1.0),
    );
    let inner = rect.w - 16.0;
    let base = rect.y + rect.h - 10.0;
    let (bottom_line, color) = match typing(app) {
        Some(text) => (tail(&text, inner), WHITE),
        None => (hint.to_string(), GRAY),
    };
    draw_text(&bottom_line, rect.x + 8.0, base, SIZE, color);
    let mut y = base - LINE_H - 4.0;
    for line in app.chat.iter().rev() {
        for piece in wrap(&shown(line), inner).iter().rev() {
            if y < rect.y + LINE_H {
                return;
            }
            draw_text(piece, rect.x + 8.0, y, SIZE, tint(line, 1.0));
            y -= LINE_H;
        }
    }
}

/// Draws the chat over the field: the line being typed with its bottom at
/// `bottom`, and above it the newest few lines, each fading out once it is
/// [`FIELD_LIFE`] old. While a line is being typed they all stay.
pub fn draw_over_field(app: &App, x: f32, bottom: f32, width: f32) {
    let open = app.draft.is_some();
    let mut y = bottom - 8.0;
    if let Some(text) = typing(app) {
        draw_rectangle(
            x,
            bottom - 30.0,
            width,
            30.0,
            Color::new(0.0, 0.0, 0.0, 0.75),
        );
        draw_rectangle_lines(x, bottom - 30.0, width, 30.0, 1.0, GRAY);
        draw_text(tail(&text, width - 16.0), x + 8.0, y, 20.0, WHITE);
        y -= 38.0;
    }
    let mut drawn = 0;
    for line in app.chat.iter().rev() {
        if !open && line.age >= FIELD_LIFE {
            return;
        }
        let alpha = if open {
            1.0
        } else {
            ((FIELD_LIFE - line.age) / 2.0).clamp(0.0, 1.0)
        };
        for piece in wrap(&shown(line), width).iter().rev() {
            if drawn >= FIELD_LINES {
                return;
            }
            let shadow = Color::new(0.0, 0.0, 0.0, alpha * 0.8);
            draw_text(piece, x + 1.0, y + 1.0, SIZE, shadow);
            draw_text(piece, x, y, SIZE, tint(line, alpha));
            y -= LINE_H;
            drawn += 1;
        }
    }
}

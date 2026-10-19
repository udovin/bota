//! The art, turned from drawings into something the screen can take.
//!
//! Each file draws its picture inside one fixed frame of a wider canvas; what
//! is rasterised is that frame alone, once for each size it is asked at, the
//! first time it is.

use macroquad::prelude::*;

/// The frame the picture is drawn in, inside the canvas of the file.
const FRAME: (f32, f32, f32, f32) = (220.0, 50.0, 240.0, 160.0);

/// How many pixels one unit of that frame is rasterised to for an icon.
pub const ICON_SCALE: f32 = 0.8;

/// How many pixels one unit of that frame is rasterised to for a hero's
/// portrait.
pub const PORTRAIT_SCALE: f32 = 1.5;

/// The texture of one drawing at an icon's size.
///
/// The first call for a drawing rasterises it; every call after hands back the
/// same texture.
pub fn icon(art: &'static [u8]) -> Option<Texture2D> {
    drawn(art, ICON_SCALE)
}

/// The texture of one drawing at a portrait's size.
pub fn portrait(art: &'static [u8]) -> Option<Texture2D> {
    drawn(art, PORTRAIT_SCALE)
}

/// The texture of one drawing at one scale, rasterised on the first call.
fn drawn(art: &'static [u8], scale: f32) -> Option<Texture2D> {
    type Drawn = Vec<((usize, u32), Option<Texture2D>)>;
    thread_local! {
        static DRAWN: std::cell::RefCell<Drawn> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let key = (art.as_ptr() as usize, scale.to_bits());
    DRAWN.with(|slot| {
        let mut drawn = slot.borrow_mut();
        if let Some((_, texture)) = drawn.iter().find(|(seen, _)| *seen == key) {
            return texture.clone();
        }
        let texture = rasterise(art, scale);
        drawn.push((key, texture.clone()));
        texture
    })
}

/// Turns one drawing into a texture of its frame.
fn rasterise(art: &[u8], scale: f32) -> Option<Texture2D> {
    let (w, h, bytes) = pixels_at(art, scale)?;
    let image = Image {
        bytes,
        width: w as u16,
        height: h as u16,
    };
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Linear);
    Some(texture)
}

/// The frame of one drawing at a scale, as width, height and straight-alpha
/// RGBA.
pub fn pixels_at(art: &[u8], scale: f32) -> Option<(u32, u32, Vec<u8>)> {
    let tree = resvg::usvg::Tree::from_data(art, &resvg::usvg::Options::default()).ok()?;
    let (w, h) = ((FRAME.2 * scale) as u32, (FRAME.3 * scale) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    // The frame is moved to the corner and scaled, so what lands on the pixmap
    // is the picture and none of the canvas around it.
    let put = resvg::tiny_skia::Transform::from_row(
        scale,
        0.0,
        0.0,
        scale,
        -FRAME.0 * scale,
        -FRAME.1 * scale,
    );
    resvg::render(&tree, put, &mut pixmap.as_mut());
    let mut bytes = pixmap.data().to_vec();
    // What comes out is multiplied by its own alpha; what goes to the screen
    // is not.
    for pixel in bytes.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in pixel.iter_mut().take(3) {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    Some((w, h, bytes))
}

/// Draws an item's icon to fill a box, or reports that there is none to draw.
pub fn draw_item_icon(id: u16, x: f32, y: f32, w: f32, h: f32) -> bool {
    let Some(texture) = crate::catalog::item(id)
        .and_then(|face| face.icon)
        .and_then(icon)
    else {
        return false;
    };
    draw_texture_ex(
        &texture,
        x,
        y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(w, h)),
            ..Default::default()
        },
    );
    true
}

/// Draws a hero's portrait to fill a box, or reports that there is none to
/// draw. A box of another shape than the portrait shows its middle, cut to
/// the box's shape.
pub fn draw_hero_portrait(hero: u16, x: f32, y: f32, w: f32, h: f32, tint: Color) -> bool {
    let Some(texture) = crate::catalog::hero(hero)
        .and_then(|face| face.icon)
        .and_then(portrait)
    else {
        return false;
    };
    let source = middle_cut(texture.width(), texture.height(), w / h.max(1.0));
    draw_texture_ex(
        &texture,
        x,
        y,
        tint,
        DrawTextureParams {
            dest_size: Some(vec2(w, h)),
            source: Some(source),
            ..Default::default()
        },
    );
    true
}

/// The middle of a picture cut to a width-to-height ratio: as tall as the
/// picture when the ratio is narrower than it, as wide when wider.
pub fn middle_cut(width: f32, height: f32, ratio: f32) -> Rect {
    if ratio <= 0.0 || width <= 0.0 || height <= 0.0 {
        return Rect::new(0.0, 0.0, width, height);
    }
    if ratio < width / height {
        let cut = height * ratio;
        Rect::new((width - cut) / 2.0, 0.0, cut, height)
    } else {
        let cut = width / ratio;
        Rect::new(0.0, (height - cut) / 2.0, width, cut)
    }
}

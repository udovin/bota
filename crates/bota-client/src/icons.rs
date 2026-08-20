//! The item art, turned from drawings into something the screen can take.
//!
//! Each file draws its icon inside one fixed frame of a wider canvas; what is
//! rasterised is that frame alone, once, the first time it is asked for.

use macroquad::prelude::*;

/// The nine drawings, in the order of the catalog.
pub const ART: [&[u8]; 9] = [
    include_bytes!("../assets/items/boots.svg"),
    include_bytes!("../assets/items/clarity.svg"),
    include_bytes!("../assets/items/healing_salve.svg"),
    include_bytes!("../assets/items/iron_branch.svg"),
    include_bytes!("../assets/items/observer_ward.svg"),
    include_bytes!("../assets/items/quelling_blade.svg"),
    include_bytes!("../assets/items/sentry_ward.svg"),
    include_bytes!("../assets/items/tango.svg"),
    include_bytes!("../assets/items/town_portal_scroll.svg"),
];

/// The frame the icon is drawn in, inside the canvas of the file.
const FRAME: (f32, f32, f32, f32) = (220.0, 50.0, 240.0, 160.0);

/// How many pixels one unit of that frame is rasterised to.
const SCALE: f32 = 0.8;

/// The icon of one item, or nothing for an id the catalog does not hold.
///
/// The first call for an id draws it; every call after hands back the same
/// texture.
pub fn item_icon(id: u16) -> Option<Texture2D> {
    thread_local! {
        static DRAWN: std::cell::RefCell<Vec<Option<Texture2D>>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    let index = usize::from(id);
    let art = ART.get(index)?;
    DRAWN.with(|slot| {
        let mut drawn = slot.borrow_mut();
        if drawn.is_empty() {
            drawn.resize(ART.len(), None);
        }
        if drawn[index].is_none() {
            drawn[index] = rasterise(art);
        }
        drawn[index].clone()
    })
}

/// Turns one drawing into a texture of its icon frame.
fn rasterise(art: &[u8]) -> Option<Texture2D> {
    let (w, h, bytes) = pixels(art)?;
    let image = Image {
        bytes,
        width: w as u16,
        height: h as u16,
    };
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Linear);
    Some(texture)
}

/// The icon frame of one drawing as width, height and straight-alpha RGBA.
pub fn pixels(art: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let tree = resvg::usvg::Tree::from_data(art, &resvg::usvg::Options::default()).ok()?;
    let (w, h) = ((FRAME.2 * SCALE) as u32, (FRAME.3 * SCALE) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    // The frame is moved to the corner and scaled, so what lands on the pixmap
    // is the icon and none of the canvas around it.
    let put = resvg::tiny_skia::Transform::from_row(
        SCALE,
        0.0,
        0.0,
        SCALE,
        -FRAME.0 * SCALE,
        -FRAME.1 * SCALE,
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
    let Some(icon) = item_icon(id) else {
        return false;
    };
    draw_texture_ex(
        &icon,
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

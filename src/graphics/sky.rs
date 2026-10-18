//! Procedural sky: a vertical gradient plus a sun disc, drawn in screen
//! space *before* the 3D pass (Camera2D disables depth test/write, so it
//! behaves as a background). Zero textures, zero shaders.

use macroquad::prelude::*;

use crate::camera::FirstPersonCamera;

/// Number of horizontal bands used to fake the gradient.
const BANDS: u32 = 24;

/// Fixed world-space direction the sun shines from.
const SUN_DIR: (f32, f32, f32) = (0.55, 0.42, -0.72);

/// Draws the sky into the current target.
///
/// `width` / `height` are the pixel dimensions of the **target being
/// drawn into** (the low-res render target), *not* the window.
///
/// This distinction is the whole point of the signature: macroquad's
/// `screen_width()` / `screen_height()` always return the *window* size,
/// even when a camera has a `render_target` attached (only `Camera3D`'s
/// projection internally consults the target size). Using screen-space
/// here would size the gradient for the window (e.g. 1280x720) while the
/// `Camera2D` maps 1 world unit to 1 *target* texel (640x360) - the sky
/// would only cover a quarter of the frame and leave the rest black.
pub fn draw(camera: &FirstPersonCamera, width: f32, height: f32) {
    // Vertical gradient: deep blue zenith -> pale, warm horizon.
    // `ceil` + a one-pixel overlap guarantees full coverage even if the
    // target height is not an exact multiple of BANDS.
    let zenith = Color::from_rgba(72, 134, 220, 255);
    let horizon = Color::from_rgba(198, 222, 238, 255);
    let band_h = (height / BANDS as f32).ceil() + 1.0;
    for i in 0..BANDS {
        let t = i as f32 / BANDS as f32; // 0 = top of the target
        let y = t * height;
        draw_rectangle(0.0, y, width, band_h, lerp_color(zenith, horizon, t));
    }

    // Sun: project its fixed world direction through the FPS camera and
    // draw it only when it is in front of the viewer. The focal length is
    // derived from the *target* height so the disc keeps its intended
    // apparent size on the low-res buffer.
    let sun = vec3(SUN_DIR.0, SUN_DIR.1, SUN_DIR.2).normalize();
    let denom = sun.dot(camera.front);
    if denom > 0.05 {
        // Focal length implied by the 60 deg vertical FOV of the camera
        let focal = 0.5 * height / (30.0f32.to_radians()).tan();
        let sx = width * 0.5 + (sun.dot(camera.right) / denom) * focal;
        let sy = height * 0.5 - (sun.dot(camera.up) / denom) * focal;
        // Scale the glow radii with the target height (they were tuned for
        // a 720px-tall buffer; the target is shorter).
        let s = height / 720.0;
        // Soft glow + bright disc
        draw_circle(sx, sy, 46.0 * s, Color::from_rgba(255, 240, 190, 45));
        draw_circle(sx, sy, 26.0 * s, Color::from_rgba(255, 248, 220, 120));
        draw_circle(sx, sy, 16.0 * s, Color::from_rgba(255, 253, 240, 255));
    }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let ch = |x: f32, y: f32| (x + (y - x) * t).clamp(0.0, 255.0) as u8;
    Color::from_rgba(
        ch(a.r * 255.0, b.r * 255.0),
        ch(a.g * 255.0, b.g * 255.0),
        ch(a.b * 255.0, b.b * 255.0),
        255,
    )
}

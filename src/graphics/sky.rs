//! Procedural sky: a vertical gradient plus a sun disc, drawn in screen
//! space *before* the 3D pass (Camera2D disables depth test/write, so it
//! behaves as a background). Zero textures, zero shaders.

use macroquad::prelude::*;

use crate::camera::FirstPersonCamera;

/// Number of horizontal bands used to fake the gradient.
const BANDS: u32 = 24;

/// Fixed world-space direction the sun shines from.
const SUN_DIR: (f32, f32, f32) = (0.55, 0.42, -0.72);

/// Draws the sky into the current (screen-space) target.
pub fn draw(camera: &FirstPersonCamera) {
    // Vertical gradient: deep blue zenith -> pale, warm horizon.
    let zenith = Color::from_rgba(72, 134, 220, 255);
    let horizon = Color::from_rgba(198, 222, 238, 255);
    let band_h = screen_height() / BANDS as f32 + 1.0;
    for i in 0..BANDS {
        let t = i as f32 / BANDS as f32; // 0 = top of the screen
        let y = t * screen_height();
        draw_rectangle(0.0, y, screen_width(), band_h, lerp_color(zenith, horizon, t));
    }

    // Sun: project its fixed world direction through the FPS camera and
    // draw it only when it is in front of the viewer.
    let sun = vec3(SUN_DIR.0, SUN_DIR.1, SUN_DIR.2).normalize();
    let denom = sun.dot(camera.front);
    if denom > 0.05 {
        // Focal length implied by the 60 deg vertical FOV of the camera
        let focal = 0.5 * screen_height() / (30.0f32.to_radians()).tan();
        let sx = screen_width() * 0.5 + (sun.dot(camera.right) / denom) * focal;
        let sy = screen_height() * 0.5 - (sun.dot(camera.up) / denom) * focal;
        // Soft glow + bright disc
        draw_circle(sx, sy, 46.0, Color::from_rgba(255, 240, 190, 45));
        draw_circle(sx, sy, 26.0, Color::from_rgba(255, 248, 220, 120));
        draw_circle(sx, sy, 16.0, Color::from_rgba(255, 253, 240, 255));
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

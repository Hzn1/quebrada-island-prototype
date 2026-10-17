use macroquad::prelude::*;
use std::time::Instant;

// Module declarations make submodules accessible via crate:: paths
mod camera;
mod config;
mod graphics;
mod player;
mod procedural;

use crate::camera::FirstPersonCamera;
use crate::config::config::PLAYER_HEIGHT;
// use crate::graphics::crt_filter::CrtFilter;
use crate::player::Player;
use crate::procedural::terrain::Terrain;

// Window configuration function
fn window_conf() -> Conf {
    Conf {
        window_title: "Quebrada Island - Voxel Engine".to_string(),
        window_width: 1280,
        window_height: 720,
        fullscreen: false,
        platform: macroquad::miniquad::conf::Platform {
            swap_interval: Some(0),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Initialize game objects
    let mut terrain = Terrain::new(50, 50);
    let mut player = Player::new(Vec3::new(0.0, 2.0, 0.0));
    let mut camera = FirstPersonCamera::new(Vec3::new(0.0, 2.0, 0.0));

    // Capture and hide the mouse cursor for FPS look
    set_cursor_grab(true);
    show_mouse(false);

    // CRT filter at lower resolution for performance on modest hardware
    // let mut crt_filter = CrtFilter::new(640, 360);

    // Frame pacing variables
    let mut last_instant = Instant::now();
    let mut accumulator = 0.0f32;
    let fixed_dt = 1.0 / 60.0;

    loop {
        // --- Frame Pacing: calculate elapsed time and cap at 60 FPS ---
        let now = Instant::now();
        let elapsed = now.duration_since(last_instant).as_secs_f32();
        last_instant = now;
        accumulator += elapsed;

        // Get mouse delta once per frame
        let m_delta = mouse_delta_position();

        // --- Update logic at fixed timestep ---
        while accumulator >= fixed_dt {
            // Handle input and update player physics
            player.handle_input(fixed_dt);
            player.apply_physics(fixed_dt, 0.0);
            player.update(fixed_dt, 0.0);

            // Update camera position to follow player
            camera.position = player.position + Vec3::Y * (PLAYER_HEIGHT / 2.0);

            // Apply mouse deltas to camera for looking around
            camera.update_from_mouse(m_delta.x, m_delta.y);

            accumulator -= fixed_dt;
        }

        // --- Render ---
        // Begin CRT render target (commented out for now)
        // crt_filter.begin_render();

        // Clear the screen (black)
        clear_background(BLACK);

        // Activate the 3D camera render pipeline
        set_camera(&camera.get_macroquad_camera());

        // Render the terrain
        terrain.render();

        // Reset to default 2D camera for UI/HUD (if any)
        set_default_camera();

        // End render target and draw to screen with CRT effect
        // crt_filter.end_render();
        // crt_filter.draw_to_screen();

        // Present the next frame
        next_frame().await;
    }
}

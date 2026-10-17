use macroquad::prelude::*;
use std::time::Instant;

// Module declarations make submodules accessible via crate:: paths
mod config;
mod player;
mod camera;
mod procedural;
mod graphics;

use crate::player::Player;
use crate::procedural::terrain::Terrain;
use crate::procedural::tree::generate_tree;
use crate::procedural::shack::generate_shack;
use crate::graphics::crt_filter::CrtFilter;

// Window configuration function
fn window_conf() -> Conf {
    Conf {
        window_title: "Quebrada Island - Voxel Engine".to_string(),
        window_width: 1280,
        window_height: 720,
        fullscreen: false,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Initialize game objects
    let terrain = Terrain::new(50, 50);

    // Spawn the player standing on the terrain surface (not inside it).
    let spawn_y = terrain.get_height_at(0.0, 0.0) + config::PLAYER_HEIGHT / 2.0;
    let mut player = Player::new(vec3(0.0, spawn_y, 0.0));

    // Capture and hide the mouse cursor for FPS look
    set_cursor_grab(true);
    show_mouse(false);

    // CRT filter: the scene is rendered into a 640x360 target (cheaper) and
    // then upscaled to the screen with the CRT shader.
    let crt_filter = CrtFilter::new(640, 360);

    // Frame pacing variables
    let mut last_instant = Instant::now();
    let mut accumulator = 0.0f32;
    let fixed_dt = 1.0 / 60.0;

    loop {
        // --- Frame pacing: calculate elapsed time and sleep to cap at 60 FPS ---
        let now = Instant::now();
        let elapsed = now.duration_since(last_instant).as_secs_f32();
        last_instant = now;
        // Clamp the accumulated time to avoid a "spiral of death" after a long
        // stall (e.g. while dragging the window).
        accumulator = (accumulator + elapsed).min(0.25);

        // --- Mouse look (once per frame; it does not need a fixed timestep) ---
        // macroquad's mouse_delta_position() is normalised to [-1, 1] and its
        // sign is inverted (it returns previous - current), so convert it back
        // to raw mouse counts and apply our conventions:
        //   positive yaw   = turn right
        //   positive pitch = look up
        let mouse = mouse_delta_position();
        let yaw_delta = -mouse.x * screen_width() * 0.5 * config::MOUSE_SENSITIVITY;
        let pitch_delta = mouse.y * screen_height() * 0.5 * config::MOUSE_SENSITIVITY;
        player.camera.update_from_mouse(yaw_delta, pitch_delta);

        // --- Update logic at fixed timestep ---
        while accumulator >= fixed_dt {
            // Player::update() runs input + physics using player.camera.front /
            // player.camera.right, so movement follows where the player looks.
            let ground_height = terrain.get_height_at(player.position.x, player.position.z);
            player.update(fixed_dt, ground_height);
            accumulator -= fixed_dt;
        }

        // --- Render ---
        // Render the scene into the CRT filter's low-res target (640x360),
        // using the first-person camera
        crt_filter.begin_render(player.camera.get_macroquad_camera());

        // Clear the target (black)
        clear_background(BLACK);

        // Draw the scene (terrain)
        terrain.render();

        // Upscale the target to the screen with the CRT shader applied
        crt_filter.draw_to_screen();

        // Frame pacing: sleep if we finished the frame early to maintain 60 FPS
        let frame_duration = now.elapsed();
        let target_duration = std::time::Duration::from_secs_f32(1.0 / 60.0);
        if frame_duration < target_duration {
            let remaining = target_duration - frame_duration;
            std::thread::sleep(remaining);
        }

        // Present the next frame
        next_frame().await;
    }
}

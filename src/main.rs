use macroquad::prelude::*;
use std::time::Instant;

// Module declarations make submodules accessible via crate:: paths
mod camera;
mod config;
mod graphics;
mod player;
mod procedural;
mod world;

use crate::graphics::crt_filter::CrtFilter;
use crate::graphics::sky;
use crate::player::Player;
use crate::world::World;

// Window configuration function
fn window_conf() -> macroquad::conf::Conf {
    let mut conf = macroquad::conf::Conf::default();
    conf.miniquad_conf.window_title = "Quebrada Island - Voxel Engine".to_string();
    conf.miniquad_conf.window_width = 1280;
    conf.miniquad_conf.window_height = 720;
    conf.miniquad_conf.fullscreen = false;

    // Disable vertical sync (vsync).
    //
    // With vsync enabled the buffer swap waits for the monitor's vblank. If a
    // frame takes even slightly longer than one refresh period, the driver
    // misses that deadline and waits for the next one, which halves the frame
    // rate (60 Hz -> 30 FPS). `swap_interval = Some(0)` tells the driver to
    // present the frame immediately instead of waiting for the vblank.
    conf.miniquad_conf.platform.swap_interval = Some(0);

    // macroquad batches geometry into fixed-size CPU/GPU buffers created at
    // startup. The defaults (10k vertices / 5k indices) would silently
    // TRUNCATE our voxel chunk meshes, so raise them to cover a full chunk
    // (u16 index limit = 65536 vertices). Bigger batches also mean fewer
    // draw calls: several chunks merge into a single one.
    conf.draw_call_vertex_capacity = 65_536;
    conf.draw_call_index_capacity = 98_304;

    conf
}

async fn run() {
    // Initialize the procedural world (infinite island streamed in chunks)
    let mut world = World::new(config::WORLD_SEED);

    // Spawn on dry, flat ground near the island centre
    let spawn = world.find_spawn();
    let mut player = Player::new(spawn);

    // Generate the chunks around the spawn up-front (one-time cost before
    // the loop starts) so the first frames are hitch-free.
    world.warm_up(player.position, config::WARMUP_RADIUS);

    // Capture and hide the mouse cursor for FPS look
    set_cursor_grab(true);
    show_mouse(false);

    // CRT filter: the scene is rendered into a 640x360 target (cheaper) and
    // then upscaled to the screen with the CRT shader.
    let crt_filter = CrtFilter::new(640, 360);

    // Frame pacing variables
    let mut last_instant = Instant::now();
    let mut accumulator = 0.0f32;
    let fixed_dt = config::FRAME_TIME_SEC;

    loop {
        // --- Frame timing: measure the delta and feed the fixed-step accumulator ---
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

        // --- Input sampled once per frame ---
        // is_key_pressed() is edge-triggered and must be read every frame: the
        // fixed-step loop below can run zero times in a given frame at high FPS.
        if is_key_pressed(KeyCode::Space) {
            player.request_jump();
        }

        // --- Update logic at fixed timestep ---
        while accumulator >= fixed_dt {
            // Height queries are pure terrain functions: they never depend on
            // chunk streaming, so physics stays stable at any speed.
            let ground_height = world.get_height_at(player.position.x, player.position.z);
            let wall = |x: f32, z: f32| world.wall_at(x, z);
            player.update(fixed_dt, ground_height, &wall);
            accumulator -= fixed_dt;
        }

        // --- World streaming: generate/unload chunks around the player ---
        // Budgeted (config::CHUNKS_PER_FRAME, nearest first) so this can
        // never cause an FPS spike while walking into new terrain.
        world.update(player.position);

        // --- Render ---
        // Pass 1: screen-space sky into the low-res target (behind everything)
        crt_filter.begin_ui_pass();
        clear_background(BLACK);
        let (target_w, target_h) = crt_filter.target_size();
        sky::draw(&player.camera, target_w, target_h);

        // Pass 2: 3D scene with the first-person camera
        crt_filter.begin_render(player.camera.get_macroquad_camera());
        world.render(
            player.camera.position,
            player.camera.front,
            get_time() as f32,
        );

        // Upscale the target to the screen with the CRT shader applied
        crt_filter.draw_to_screen();

        // --- HUD (screen space, drawn on top of the CRT pass) ---
        // FPS + streaming stats in the top-left corner. A dark shadow
        // behind keeps it readable over bright terrain.
        let hud_text = format!("FPS: {} | chunks: {}", get_fps(), world.chunk_count());
        draw_text(&hud_text, 11.0, 27.0, 26.0, BLACK);
        draw_text(&hud_text, 10.0, 26.0, 26.0, WHITE);

        // Frame pacing: optionally cap the frame rate (see config::TARGET_FPS).
        // Vsync is disabled in `window_conf`, so with the cap set to 0 the loop
        // simply runs as fast as the machine can render.
        if config::TARGET_FPS > 0 {
            let frame_duration = now.elapsed();
            let target_duration =
                std::time::Duration::from_secs_f32(1.0 / config::TARGET_FPS as f32);
            if frame_duration < target_duration {
                std::thread::sleep(target_duration - frame_duration);
            }
        }

        // Present the next frame
        next_frame().await;
    }
}

/// Real entry point: `#[macroquad::main]` is not used because we need to
/// customize `draw_call_vertex_capacity` / `draw_call_index_capacity`,
/// which only `Window::from_config` exposes.
fn main() {
    macroquad::Window::from_config(window_conf(), run());
}

/// Global game configuration constants.
///
/// Render frame-rate cap.
/// `0` means "no cap" (render as fast as the machine can, since vsync is
/// disabled); set it to e.g. `60` or `144` to limit the frame rate again.
pub const TARGET_FPS: u32 = 0;
/// Fixed timestep used for the physics/input simulation (always 60 Hz,
/// independent of the render frame rate).
pub const FRAME_TIME_SEC: f32 = 1.0 / 60.0;

/// Walking speed in units per second.
pub const WALK_SPEED: f32 = 5.0;
/// Running speed (shift multiplier) in units per second.
pub const RUN_SPEED: f32 = 10.0;
/// Jump force applied when the player jumps.
pub const JUMP_FORCE: f32 = 8.0;
/// Gravity acceleration applied each frame.
pub const GRAVITY: f32 = 15.0;
/// Player height above ground.
pub const PLAYER_HEIGHT: f32 = 2.0;

/// Mouse sensitivity for camera movement.
pub const MOUSE_SENSITIVITY: f32 = 0.005;

/// Size of a single voxel in world units.
pub const VOXEL_SIZE: f32 = 1.0;
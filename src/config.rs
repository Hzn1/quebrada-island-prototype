/// Global game configuration constants.
pub const TARGET_FPS: u32 = 60;
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
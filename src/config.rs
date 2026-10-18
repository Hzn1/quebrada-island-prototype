/// Global game configuration constants.
///
/// Render frame-rate cap.
/// `0` means "no cap" (render as fast as the machine can, since vsync is
/// disabled); set it to e.g. `60` or `144` to limit the frame rate again.
pub const TARGET_FPS: u32 = 0;
/// Fixed timestep used for the physics/input simulation (always 60 Hz,
/// independent of the render frame rate).
pub const FRAME_TIME_SEC: f32 = 1.0 / 60.0;

// ---------------------------------------------------------------------------
// Procedural world
// ---------------------------------------------------------------------------

/// Seed of the island: change it to get a completely different world.
pub const WORLD_SEED: u64 = 20_251_017;
/// Water level. Ground quantized to `SEA_LEVEL` is the shoreline.
pub const SEA_LEVEL: f32 = 7.0;
/// Width (in blocks, above sea level) of the sand strip around the coast.
pub const BEACH_WIDTH: f32 = 2.5;
/// Approximate radius of the island in world units.
pub const ISLAND_RADIUS: f32 = 110.0;

/// Side length of one chunk in blocks (must stay well below the u16
/// vertex-index limit of the mesher).
pub const CHUNK_SIZE: i32 = 32;
/// Radius, in chunks, of the area that is meshed and rendered around the
/// player (circular). 4 chunks = 128 blocks of view distance.
pub const RENDER_DISTANCE: i32 = 4;
/// How many new chunks are allowed to be generated per frame (nearest
/// first). Keeps streaming from causing FPS spikes.
pub const CHUNKS_PER_FRAME: usize = 2;
/// Chunks generated synchronously at startup around the spawn point.
pub const WARMUP_RADIUS: i32 = 2;

/// Side length of the decoration grid in blocks: one spawn candidate per
/// cell keeps decorations evenly spread and physically unable to overlap.
pub const DECOR_CELL_SIZE: i32 = 6;
/// Base probability that a candidate cell grows a tree (scaled by local
/// humidity, so humid cells become forest).
pub const TREE_DENSITY: f32 = 0.35;
/// Base probability that a candidate cell gets a rock formation.
pub const ROCK_DENSITY: f32 = 0.10;

/// Walking speed in units per second.
pub const WALK_SPEED: f32 = 5.0;
/// Running speed (shift multiplier) in units per second.
pub const RUN_SPEED: f32 = 10.0;
/// Jump force applied when the player jumps (initial upward velocity, u/s).
/// With the current GRAVITY this gives a jump height of about 1.3 voxels.
pub const JUMP_FORCE: f32 = 8.0;
/// Gravity acceleration in units per second squared. Real gravity is ~9.8, but
/// games usually use a higher value so falling feels immediate instead of
/// floaty ("moon" feel).
pub const GRAVITY: f32 = 25.0;
/// Player height above ground.
pub const PLAYER_HEIGHT: f32 = 2.0;

/// Mouse sensitivity for camera movement.
pub const MOUSE_SENSITIVITY: f32 = 0.005;

/// Size of a single voxel in world units.
pub const VOXEL_SIZE: f32 = 1.0;
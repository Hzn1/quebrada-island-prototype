use macroquad::prelude::*;
use crate::config;
use crate::camera::FirstPersonCamera;

/// How long (in seconds) a queued jump stays valid. Buffers the jump input so
/// a press is never lost on a frame that runs zero fixed-size physics steps
/// (possible now that the frame rate is uncapped).
const JUMP_BUFFER_TIME: f32 = 0.15;

/// Player struct holding position, velocity, grounding state, and camera.
#[derive(Clone)]
pub struct Player {
    pub position: Vec3,
    pub velocity: Vec3,
    pub is_grounded: bool,
    pub camera: FirstPersonCamera,
    /// Time left for a queued jump to stay valid (see `JUMP_BUFFER_TIME`).
    jump_buffer: f32,
}

impl Player {
    /// Creates a new player at the given spawn position.
    pub fn new(spawn_pos: Vec3) -> Self {
        let camera = FirstPersonCamera::new(spawn_pos + Vec3::Y * 2.0);
        Self {
            position: spawn_pos,
            velocity: Vec3::ZERO,
            is_grounded: true,
            camera,
            jump_buffer: 0.0,
        }
    }

    /// Queues a jump request.
    ///
    /// Call this once per frame when the jump key is pressed. The request stays
    /// valid for `JUMP_BUFFER_TIME` so it is not lost when a frame runs no
    /// fixed-size physics step.
    pub fn request_jump(&mut self) {
        self.jump_buffer = JUMP_BUFFER_TIME;
    }

    /// Handles the movement keys and sets the player's horizontal velocity.
    /// The jump input is handled separately through `request_jump`, because it
    /// is edge-triggered and must be sampled once per frame.
    /// _delta_time is in seconds (unused, kept for signature consistency).
    pub fn handle_input(&mut self, _delta_time: f32) {
        // Determine movement speed based on shift key (run modifier)
        let walk_speed = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
            config::RUN_SPEED
        } else {
            config::WALK_SPEED
        };

        // Calculate forward/backward input (+1 for W, -1 for S, 0 for none)
        let forward_input = if is_key_down(KeyCode::W) { 1.0 } else { 0.0 }
            - if is_key_down(KeyCode::S) { 1.0 } else { 0.0 };
        
        // Calculate left/right input (+1 for D, -1 for A, 0 for none)
        let right_input = if is_key_down(KeyCode::D) { 1.0 } else { 0.0 }
            - if is_key_down(KeyCode::A) { 1.0 } else { 0.0 };

        // Get the camera's forward and right vectors (already normalized)
        let forward = self.camera.front;
        let right = self.camera.right;

        // Create flat vectors (no Y component) for horizontal movement
        let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let right_flat = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

        // Calculate movement direction in world space
        let mut move_dir = Vec3::ZERO;
        move_dir += forward_flat * forward_input;
        move_dir += right_flat * right_input;

        // Normalize to prevent faster diagonal movement
        if move_dir.length_squared() > 0.0 {
            move_dir = move_dir.normalize();
        }

        // Apply the horizontal speed only. The vertical velocity belongs to the
        // physics (gravity/jump); overwriting it here would reset the fall speed
        // to zero on every step, making the player float down in slow motion.
        self.velocity.x = move_dir.x * walk_speed;
        self.velocity.z = move_dir.z * walk_speed;
    }

    /// Applies physics (gravity) and resolves collisions.
    ///
    /// `ground_height` is the walkable terrain surface at the player's column;
    /// `wall` returns the vertical span of the building occupying a column
    /// (`None` when open). Horizontal movement is attempted one axis at a
    /// time, so a blocked axis slides along the wall instead of stopping the
    /// player dead - which is what makes 1-block-wide alleys pleasant to walk.
    pub fn apply_physics(
        &mut self,
        delta_time: f32,
        ground_height: f32,
        wall: &dyn Fn(f32, f32) -> Option<(i32, i32)>,
    ) {
        // Apply gravity if not grounded
        if !self.is_grounded {
            self.velocity.y -= config::GRAVITY * delta_time;
        }

        let half_height = config::PLAYER_HEIGHT / 2.0;
        let feet = self.position.y - half_height;

        // Horizontal movement, axis by axis: houses block, open columns do not.
        let next_x = self.position.x + self.velocity.x * delta_time;
        if blocked(wall, next_x, self.position.z, feet) {
            self.velocity.x = 0.0;
        } else {
            self.position.x = next_x;
        }
        let next_z = self.position.z + self.velocity.z * delta_time;
        if blocked(wall, self.position.x, next_z, feet) {
            self.velocity.z = 0.0;
        } else {
            self.position.z = next_z;
        }

        // Vertical movement + ground collision
        self.position.y += self.velocity.y * delta_time;
        let half_height = config::PLAYER_HEIGHT / 2.0;
        let feet = self.position.y - half_height;
        // Small epsilon so that standing exactly on the surface still counts as
        // grounded; otherwise floating-point rounding would make the player
        // alternate between grounded and airborne while walking on flat ground
        // (and the edge-triggered jump would be dropped half the time).
        if feet <= ground_height + 0.01 {
            self.position.y = ground_height + half_height;
            if self.velocity.y < 0.0 {
                self.velocity.y = 0.0;
            }
            self.is_grounded = true;
        } else {
            self.is_grounded = false;
        }
    }

    /// Main update step: handle input, consume a buffered jump, then physics.
    pub fn update(
        &mut self,
        delta_time: f32,
        ground_height: f32,
        wall: &dyn Fn(f32, f32) -> Option<(i32, i32)>,
    ) {
        self.handle_input(delta_time);

        // Jump only when actually on the ground; the buffer also covers a jump
        // pressed a moment before landing.
        if self.jump_buffer > 0.0 && self.is_grounded {
            self.velocity.y = config::JUMP_FORCE;
            self.is_grounded = false;
            self.jump_buffer = 0.0;
        }
        self.jump_buffer = (self.jump_buffer - delta_time).max(0.0);

        self.apply_physics(delta_time, ground_height, wall);
        // Update the camera's position to match the player's eye level
        self.camera.position = self.position + Vec3::Y * (config::PLAYER_HEIGHT / 2.0);
    }
}

/// True when a building occupies column `(x, z)` at the player's body height,
/// i.e. moving there would push the player into a wall. Blocks that sit
/// entirely below the feet (a roof) or fully above the head (an overhang)
/// leave the column passable.
fn blocked(wall: &dyn Fn(f32, f32) -> Option<(i32, i32)>, x: f32, z: f32, feet: f32) -> bool {
    match wall(x, z) {
        Some((lo, hi)) => hi as f32 >= feet && lo as f32 <= feet + config::PLAYER_HEIGHT,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_blocks_only_at_body_height() {
        // A wall occupying rows 10..=15 (blocks span [10, 16]).
        let wall = |_x: f32, _z: f32| Some((10, 15));
        assert!(blocked(&wall, 0.0, 0.0, 10.0), "feet at the base of the wall");
        assert!(blocked(&wall, 0.0, 0.0, 12.0), "body inside the wall");
        assert!(!blocked(&wall, 0.0, 0.0, 16.0), "standing on the roof");
        assert!(!blocked(&wall, 0.0, 0.0, 7.0), "wall starts above the head");
        // Open column never blocks.
        let open = |_x: f32, _z: f32| None;
        assert!(!blocked(&open, 0.0, 0.0, 3.0));
    }
}
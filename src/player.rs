use macroquad::prelude::*;
use crate::config;
use crate::camera::FirstPersonCamera;

/// Player struct holding position, velocity, grounding state, and camera.
#[derive(Clone)]
pub struct Player {
    pub position: Vec3,
    pub velocity: Vec3,
    pub is_grounded: bool,
    pub camera: FirstPersonCamera,
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
        }
    }

    /// Handles keyboard input for movement and jump input.
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

        // Apply speed
        self.velocity = move_dir * walk_speed;

        // Jump input
        if is_key_pressed(KeyCode::Space) && self.is_grounded {
            self.velocity.y = config::JUMP_FORCE; // JUMP_FORCE
            self.is_grounded = false;
        }
    }

    /// Applies physics (gravity) and checks ground collision.
    pub fn apply_physics(&mut self, delta_time: f32, ground_height: f32) {
        // Apply gravity if not grounded
        if !self.is_grounded {
            self.velocity.y -= config::GRAVITY * delta_time; // GRAVITY
        }

        // Store original position for collision detection
        let _original_position = self.position;
        
        // Apply movement
        self.position.x += self.velocity.x * delta_time;
        self.position.z += self.velocity.z * delta_time;
        self.position.y += self.velocity.y * delta_time;

        // Simple ground collision: if we're below ground, snap back up
        let player_height = config::PLAYER_HEIGHT;
        if self.position.y - player_height / 2.0 < ground_height {
            self.position.y = ground_height + player_height / 2.0;
            self.velocity.y = 0.0;
            self.is_grounded = true;
        } else {
            self.is_grounded = false;
        }
    }

    /// Main update step: handle input then apply physics.
    pub fn update(&mut self, delta_time: f32, ground_height: f32) {
        self.handle_input(delta_time);
        self.apply_physics(delta_time, ground_height);
        // Update the camera's position to match the player's eye level
        self.camera.position = self.position + Vec3::Y * (config::PLAYER_HEIGHT / 2.0);
    }
}
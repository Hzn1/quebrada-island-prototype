use crate::camera::FirstPersonCamera;
use crate::config::config;
use macroquad::prelude::*;

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
    /// delta_time is in seconds.
    pub fn handle_input(&mut self, delta_time: f32) {
        let walk_speed = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
            config::RUN_SPEED
        } else {
            config::WALK_SPEED
        };

        let forward = self.camera.front;
        let right = self.camera.right;

        // Normalize the forward vector to remove vertical component for flat movement
        let mut move_dir = Vec3::ZERO;
        move_dir.x += if is_key_down(KeyCode::W) { 1.0 } else { 0.0 };
        move_dir.z += if is_key_down(KeyCode::S) { 1.0 } else { 0.0 };
        move_dir.x += if is_key_down(KeyCode::A) { -1.0 } else { 0.0 };
        move_dir.z += if is_key_down(KeyCode::D) { -1.0 } else { 0.0 };
        move_dir = move_dir.normalize_or_zero();

        // Transform move direction into world space based on camera orientation
        let mut desired_velocity = Vec3::new(move_dir.x, 0.0, move_dir.z);
        if desired_velocity.length_squared() > 0.0 {
            // Project onto the horizontal plane using camera right/front
            desired_velocity = desired_velocity.x * right + desired_velocity.z * self.camera.front;
            desired_velocity.y = 0.0;
            desired_velocity = desired_velocity.normalize() * walk_speed;
        }

        self.velocity = desired_velocity;

        // Jump input
        if is_key_pressed(KeyCode::Space) && self.is_grounded {
            self.velocity.y = config::JUMP_FORCE;
            self.is_grounded = false;
        }
    }

    /// Applies physics (gravity) and checks ground collision.
    pub fn apply_physics(&mut self, delta_time: f32, ground_height: f32) {
        // Apply gravity
        if !self.is_grounded {
            self.velocity.y -= config::GRAVITY * delta_time;
        }

        // Move the player on the horizontal plane
        let new_x = self.position.x + self.velocity.x * delta_time;
        let new_z = self.position.z + self.velocity.z * delta_time;

        // Simple ground collision: if the player's bottom goes below ground height, snap to ground
        let mut new_y = self.position.y + self.velocity.y * delta_time;

        // If the player goes below ground height, snap to ground and reset vertical velocity
        if new_y - (config::PLAYER_HEIGHT / 2.0) < ground_height {
            new_y = ground_height + (config::PLAYER_HEIGHT / 2.0);
            self.velocity.y = 0.0;
            self.is_grounded = true;
        }

        self.position = Vec3::new(new_x, new_y, new_z);
    }

    /// Main update step: handle input then apply physics.
    pub fn update(&mut self, delta_time: f32, ground_height: f32) {
        self.handle_input(delta_time);
        self.apply_physics(delta_time, ground_height);
        // Update the camera's position to match the player
        self.camera.position = self.position + Vec3::Y * (config::PLAYER_HEIGHT / 2.0);
    }
}

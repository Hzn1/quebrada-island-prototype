use macroquad::prelude::*;
use std::f32::consts::PI;
use crate::config;

/// First-person camera implementation for a voxel-based game.
#[derive(Clone, Debug)]
pub struct FirstPersonCamera {
    /// Position of the camera in world space.
    pub position: Vec3,
    /// Yaw angle in radians (rotation around Y-axis).
    /// 0.0 = looking along negative Z axis.
    pub yaw: f32,
    /// Pitch angle in radians (rotation around X-axis, clamped to avoid gimbal lock).
    /// 0.0 = looking straight ahead.
    pub pitch: f32,
    /// Front direction vector (normalized).
    pub front: Vec3,
    /// Right direction vector (normalized).
    pub right: Vec3,
    /// Up direction vector (normalized).
    pub up: Vec3,
}

impl FirstPersonCamera {
    /// Creates a new first-person camera at the given position.
    /// 
    /// Initializes with yaw=0, pitch=0 (looking along negative Z-axis).
    pub fn new(position: Vec3) -> Self {
        let mut camera = Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            front: Vec3::ZERO,
            right: Vec3::ZERO,
            up: Vec3::ZERO,
        };
        camera.update_vectors();
        camera
    }

    /// Updates the front, right, and up vectors based on current yaw and pitch.
    /// 
    /// This must be called whenever yaw or pitch changes.
    fn update_vectors(&mut self) {
        // Clamp pitch to prevent screen flip when looking straight up/down
        self.pitch = self.pitch.max(-PI / 2.0 + 0.01).min(PI / 2.0 - 0.01);
        
        // Calculate front vector using spherical coordinates
        // In Macroquad's right-handed coordinate system:
        // - X: right
        // - Y: up  
        // - Z: backward (so -Z is forward)
        self.front = Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        ).normalize();
        
        // Calculate right vector: front × world_up (where world_up = +Y)
        self.right = self.front.cross(Vec3::Y).normalize();
        
        // Recalculate up vector to ensure orthogonality: right × front
        self.up = self.right.cross(self.front).normalize();
    }

    /// Rotates the camera by the given yaw/pitch deltas, in radians.
    ///
    /// A positive `yaw_delta` turns the camera to the right; a positive
    /// `pitch_delta` looks up. Pitch is clamped to prevent gimbal lock.
    ///
    /// Input-device units, signs and mouse sensitivity are the caller's
    /// responsibility (see `config::MOUSE_SENSITIVITY`).
    pub fn update_from_mouse(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.yaw += yaw_delta;
        self.pitch += pitch_delta;

        // Update the directional vectors (update_vectors clamps the pitch)
        self.update_vectors();
    }

    /// Returns a Camera3D compatible with Macroquad's render pipeline.
    /// 
    /// The camera looks from `position` toward `position + front`.
    pub fn get_macroquad_camera(&self) -> Camera3D {
        let mut cam = Camera3D::default();
        cam.position = self.position;
        cam.target = self.position + self.front;
        cam.up = self.up;
        // Camera3D::fovy expects radians (Camera3D::default uses 45°.to_radians())
        cam.fovy = 60.0_f32.to_radians();
        cam
    }
}
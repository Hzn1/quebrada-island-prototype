use macroquad::prelude::*;

use crate::config::config;

/// First-person camera implementation for a voxel-based game.
#[derive(Clone)]
pub struct FirstPersonCamera {
    pub position: Vec3,
    pub yaw: f32,   // Horizontal rotation in radians
    pub pitch: f32, // Vertical rotation in radians (clamped to +/-89 degrees)
    pub front: Vec3,
    pub up: Vec3,
    pub right: Vec3,
}

impl FirstPersonCamera {
    /// Creates a new first-person camera at the given position.
    pub fn new(position: Vec3) -> Self {
        let mut camera = Self {
            position,
            yaw: -std::f32::consts::FRAC_PI_2, // Começa olhando para frente (ajuste o ângulo se precisar)
            pitch: 0.0,
            front: Vec3::default(),
            up: Vec3::default(),
            right: Vec3::default(),
        };
        // IMPORTANTE: Calcula os vetores logo na criação para não iniciarem zerados!
        camera.update_vectors(camera.yaw, camera.pitch);
        camera
    }

    /// Recalculates front/right/up from current yaw/pitch and assigns them.
    fn update_vectors(&mut self, yaw: f32, pitch: f32) {
        let pitch_clamped = pitch
            .max(-std::f32::consts::PI / 2.0)
            .min(std::f32::consts::PI / 2.0 - 0.01);
        let front_x = yaw.cos() * pitch_clamped.cos();
        let front_y = pitch_clamped.sin();
        let front_z = yaw.sin() * pitch_clamped.cos();
        self.front = Vec3::new(front_x, front_y, front_z).normalize();
        self.right = self.front.cross(Vec3::Y).normalize();
        self.up = self.right.cross(self.front).normalize();
    }

    /// Updates camera orientation from mouse delta movement.
    /// Mouse dx controls yaw, dy controls pitch with clamping.
    pub fn update_from_mouse(&mut self, mouse_dx: f32, mouse_dy: f32) {
        let sensitivity = config::MOUSE_SENSITIVITY;
        self.yaw -= mouse_dx * sensitivity;
        self.pitch += mouse_dy * sensitivity;
        // Clamp pitch to prevent flipping upside-down
        self.pitch = self
            .pitch
            .max(-std::f32::consts::PI / 2.0 + 0.01)
            .min(std::f32::consts::PI / 2.0 - 0.01);
        self.update_vectors(self.yaw, self.pitch);
    }

    /// Returns a Camera3D compatible with Macroquad's render pipeline.
    pub fn get_macroquad_camera(&self) -> Camera3D {
        Camera3D {
            position: self.position,
            target: self.position + self.front,
            up: self.up,
            fovy: 90.0,           // Campo de visão vertical em graus
            ..Default::default() // Preenche automaticamente o resto (aspect, projection, viewport, etc.)
        }
    }
}

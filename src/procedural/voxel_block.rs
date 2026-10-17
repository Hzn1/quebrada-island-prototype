use macroquad::prelude::*;

/// Enum representing different types of voxels in the world.
#[derive(Clone, PartialEq)]
pub enum VoxelType {
    Air,
    Dirt,
    Grass,
    Brick,
    Concrete,
    BluePlastic,
    Wood,
    Leaves,
}

/// A single voxel block with position, type, and color.
#[derive(Clone)]
pub struct VoxelBlock {
    pub position: Vec3,
    pub voxel_type: VoxelType,
    pub color: Color,
}

impl VoxelBlock {
    /// Returns the Color associated with a given VoxelType.
    /// Colors are hardcoded (procedural) - no external image assets.
    pub fn get_color_for_type(voxel_type: VoxelType) -> Color {
        match voxel_type {
            VoxelType::Air => Color::from_rgba(0, 0, 0, 0),
            VoxelType::Dirt => Color::from_rgba(102, 51, 0, 255),     // Brownish dirt
            VoxelType::Grass => Color::from_rgba(34, 139, 34, 255),   // Forest green grass top
            VoxelType::Brick => Color::from_rgba(184, 102, 51, 255),   // Reddish brick
            VoxelType::Concrete => Color::from_rgba(189, 189, 189, 255), // Light gray concrete
            VoxelType::BluePlastic => Color::from_rgba(25, 25, 200, 255), // Vibrant blue plastic
            VoxelType::Wood => Color::from_rgba(150, 75, 0, 255),       // Dark wood brown
            VoxelType::Leaves => Color::from_rgba(34, 139, 34, 255),    // Leaf green (same as grass for simplicity)
        }
    }
}
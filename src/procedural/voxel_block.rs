use macroquad::prelude::*;

/// Enum representing different types of voxels in the world.
/// Some variants (Air, Brick, Concrete, BluePlastic) are only used by the
/// legacy `shack` demo generator.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VoxelType {
    Air,
    Dirt,
    Grass,
    Sand,
    Stone,
    Brick,
    Concrete,
    BluePlastic,
    Wood,
    Leaves,
}

/// A single voxel block with position, type, and color.
/// (Kept for the legacy `shack` demo generator.)
#[allow(dead_code)]
#[derive(Clone)]
pub struct VoxelBlock {
    pub position: Vec3,
    pub voxel_type: VoxelType,
    pub color: Color,
}

/// A voxel to be stamped into the world during chunk generation.
/// Uses integer grid coordinates so that generation is deterministic and
/// produces identical results on both sides of a chunk border.
#[derive(Clone, Copy, Debug)]
pub struct VoxelStamp {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub kind: VoxelType,
}

impl VoxelBlock {
    /// Returns the Color associated with a given VoxelType.
    /// Colors are hardcoded (procedural) - no external image assets.
    pub fn get_color_for_type(voxel_type: VoxelType) -> Color {
        match voxel_type {
            VoxelType::Air => Color::from_rgba(0, 0, 0, 0),
            VoxelType::Dirt => Color::from_rgba(102, 51, 0, 255),      // Brownish dirt
            VoxelType::Grass => Color::from_rgba(88, 158, 62, 255),    // Fresh grass green
            VoxelType::Sand => Color::from_rgba(227, 205, 152, 255),   // Warm beach sand
            VoxelType::Stone => Color::from_rgba(126, 127, 132, 255),  // Neutral gray rock
            VoxelType::Brick => Color::from_rgba(184, 102, 51, 255),   // Reddish brick
            VoxelType::Concrete => Color::from_rgba(189, 189, 189, 255), // Light gray concrete
            VoxelType::BluePlastic => Color::from_rgba(25, 25, 200, 255), // Vibrant blue plastic
            VoxelType::Wood => Color::from_rgba(150, 75, 0, 255),      // Dark wood brown
            VoxelType::Leaves => Color::from_rgba(46, 139, 58, 255),   // Leaf green
        }
    }
}

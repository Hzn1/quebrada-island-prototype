use macroquad::prelude::*;
use crate::procedural::voxel_block::{VoxelBlock, VoxelType};

/// Generates a procedural tree as a collection of voxel blocks.
/// Creates a trunk (fixed height of 5 blocks) of Wood blocks and a leaf canopy
/// (Leaves blocks) arranged in a cube pattern around the top of the trunk.
pub fn generate_tree(base_position: Vec3) -> Vec<VoxelBlock> {
    let mut blocks = Vec::new();

    let trunk_height = 5; // fixed height: 5 Wood blocks

    // Generate trunk: vertical column of Wood blocks
    for y in 0..trunk_height {
        let pos = base_position + Vec3::Y * (y as f32);
        let block = VoxelBlock {
            position: pos,
            voxel_type: VoxelType::Wood,
            color: VoxelBlock::get_color_for_type(VoxelType::Wood),
        };
        blocks.push(block);
    }

    // Generate leaf canopy: a 3x3x3 cube of Leaves around the top of the trunk
    let canopy_center_y = trunk_height as f32; // top of trunk
    let radius = 1; // extent in each direction

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                // Only include blocks where the sum of absolute offsets is odd
                // to create a roughly spherical shape (diamond pattern)
                let manhattan = ((dx as f32).abs() + (dy as f32).abs() + (dz as f32).abs()) as f32;
                if manhattan <= 2.5 {
                    let pos = base_position
                        + Vec3::new(dx as f32, canopy_center_y as f32 + dy as f32, dz as f32);
                    let block = VoxelBlock {
                        position: pos,
                        voxel_type: VoxelType::Leaves,
                        color: VoxelBlock::get_color_for_type(VoxelType::Leaves),
                    };
                    blocks.push(block);
                }
            }
        }
    }

    blocks
}
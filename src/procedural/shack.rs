use macroquad::prelude::*;
use crate::procedural::voxel_block::{VoxelBlock, VoxelType};

/// Generates a procedural shack as a collection of voxel blocks.
/// Walls: Brick blocks.
/// Floor/ceiling: Concrete blocks.
/// Water tank on top: 2x2 BluePlastic blocks.
pub fn generate_shack(base_position: Vec3) -> Vec<VoxelBlock> {
    let mut blocks = Vec::new();

    // Dimensions of the shack (5x4 base)
    let width = 5;
    let depth = 4;

    // Generate walls: vertical columns of Brick blocks
    for x in 0..=width {
        for z in 0..=depth {
            // Skip the entrance gap on the front face (z=0, x=2)
            if x == 2 && z == 0 {
                continue;
            }
            // Front wall (z=0)
            for y in 0..3 {
                let pos = base_position + Vec3::new(x as f32, y as f32, 0.0);
                let block = VoxelBlock {
                    position: pos,
                    voxel_type: VoxelType::Brick,
                    color: VoxelBlock::get_color_for_type(VoxelType::Brick),
                };
                blocks.push(block);
            }
            // Back wall (z=depth)
            for y in 0..3 {
                let pos = base_position + Vec3::new(x as f32, y as f32, depth as f32);
                let block = VoxelBlock {
                    position: pos,
                    voxel_type: VoxelType::Brick,
                    color: VoxelBlock::get_color_for_type(VoxelType::Brick),
                };
                blocks.push(block);
            }
            // Left wall (x=0) - skip y=0 since that's the floor area already covered
            if x == 0 {
                for y in 1..3 {
                    let pos = base_position + Vec3::new(0.0, y as f32, z as f32);
                    let block = VoxelBlock {
                        position: pos,
                        voxel_type: VoxelType::Brick,
                        color: VoxelBlock::get_color_for_type(VoxelType::Brick),
                    };
                    blocks.push(block);
                }
            }
            // Right wall (x=width) - skip y=0
            if x == width {
                for y in 1..3 {
                    let pos = base_position + Vec3::new(width as f32, y as f32, z as f32);
                    let block = VoxelBlock {
                        position: pos,
                        voxel_type: VoxelType::Brick,
                        color: VoxelBlock::get_color_for_type(VoxelType::Brick),
                    };
                    blocks.push(block);
                }
            }
        }
    }

    // Generate floor: Concrete blocks covering the base area
    for x in 0..=width {
        for z in 0..=depth {
            let pos = base_position + Vec3::new(x as f32, 0.0, z as f32);
            let block = VoxelBlock {
                position: pos,
                voxel_type: VoxelType::Concrete,
                color: VoxelBlock::get_color_for_type(VoxelType::Concrete),
            };
            blocks.push(block);
        }
    }

    // Generate water tank on top of the shack roof (at y=3)
    // A 2x2 group of BluePlastic blocks at the back-left of the roof
    for dx in 0..2 {
        for dz in 0..2 {
            let pos =
                base_position + Vec3::new((width - 1 + dx) as f32, 3.0, (depth - 1 + dz) as f32);
            let block = VoxelBlock {
                position: pos,
                voxel_type: VoxelType::BluePlastic,
                color: VoxelBlock::get_color_for_type(VoxelType::BluePlastic),
            };
            blocks.push(block);
        }
    }

    blocks
}
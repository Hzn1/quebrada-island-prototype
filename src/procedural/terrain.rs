use macroquad::prelude::*;
use crate::config;
use crate::procedural::voxel_block::{VoxelBlock, VoxelType};

/// Procedural terrain generated using simple trigonometric functions.
/// Simulates a hill/mountain without needing external terrain data.
pub struct Terrain {
    /// Width of the terrain in number of voxels.
    width: usize,
    /// Depth of the terrain in number of voxels.
    depth: usize,
    /// Height map: elevation at each (x, z) coordinate.
    heights: Vec<f32>,
}

impl Terrain {
    /// Creates a new terrain with the given width and depth.
    pub fn new(width: usize, depth: usize) -> Self {
        let mut heights = Vec::with_capacity(width * depth);
        for z in 0..depth {
            for x in 0..width {
                // Combine sine waves to create rolling hills
                let xf = x as f32;
                let zf = z as f32;
                let height = 10.0
                    * (xf * 0.1).sin()
                    * (zf * 0.1).cos()
                    + 5.0 * (xf * 0.05).sin()
                    + 3.0 * (zf * 0.05).cos();
                heights.push(height.max(0.0));
            }
        }
        Self { width, depth, heights }
    }

    /// Returns the height (Y coordinate) at a given world X and Z position.
    pub fn get_height_at(&self, x: f32, z: f32) -> f32 {
        let x_idx = x.clamp(0.0, self.width as f32 - 1.0);
        let z_idx = z.clamp(0.0, self.depth as f32 - 1.0);
        let ix = x_idx as usize;
        let iz = z_idx as usize;
        let base = ix + iz * self.width;
        if base < self.heights.len() as usize {
            self.heights[base]
        } else {
            0.0
        }
    }

    /// Renders the terrain surface by drawing cubes for each voxel column
    /// up to the height map value.
    pub fn render(&self) {
        let size = config::VOXEL_SIZE;
        for z in 0..self.depth {
            for x in 0..self.width {
                let h = self.heights[x + z * self.width];
                let y = h / 2.0; // center the voxel column at half height
                // Draw a column of voxels from ground up to height
                for vy in 0..=h as i32 {
                    let Vy = vy as f32;
                    let block_type = if Vy == h as f32 {
                        // Top surface gets grass
                        VoxelType::Grass
                    } else if Vy > h as f32 - 2.0 {
                        // Upper layer: dirt
                        VoxelType::Dirt
                    } else {
                        // Lower layer: nothing below ground level
                        continue;
                    };
                    let pos = Vec3::new(
                        x as f32,
                        Vy as f32,
                        z as f32,
                    );
                    let color = VoxelBlock::get_color_for_type(block_type);
                    // Draw a single cube at this position
                    draw_cube(
                        pos + Vec3::Y * (size / 2.0),
                        Vec3::new(size, size, size),
                        None,
                        color,
                    );
                }
            }
        }
    }
}
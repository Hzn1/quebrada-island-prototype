//! Procedural vegetation shapes (trees and palms) expressed as voxel
//! stamps. Every shape is derived from a `u64` hash, so the same hash
//! always produces the same plant, anywhere in the world - this is what
//! lets different chunks agree on a tree that straddles their border.

use std::f32::consts::TAU;

use crate::procedural::noise::hash01;
use crate::procedural::voxel_block::{VoxelStamp, VoxelType};

/// Callback used by the shape generators to emit voxels.
pub type StampFn<'a> = dyn FnMut(VoxelStamp) + 'a;

/// Emits one voxel through the callback (tiny helper to keep the shape
/// generators readable).
#[inline]
fn push(emit: &mut StampFn, x: i32, y: i32, z: i32, kind: VoxelType) {
    emit(VoxelStamp::new(x, y, z, kind));
}

/// Deciduous tree: a wooden trunk with a slightly flared base and a
/// ragged, ellipsoid leaf canopy. `base_y` is the ground surface: the
/// trunk starts exactly on it.
pub fn tree(base_x: i32, base_y: i32, base_z: i32, hash: u64, emit: &mut StampFn) {
    let r = |salt: u64| hash01(0, 0, hash ^ salt);

    let trunk_h = 4 + (r(0x1_0001) * 4.0) as i32; // 4..7 blocks
    let big = trunk_h >= 6;

    // Trunk: simple 1x1 column of wood
    for y in 0..trunk_h {
        push(emit, base_x, base_y + y, base_z, VoxelType::Wood);
    }
    // Root flare on bigger trees (also hides the trunk/ground seam)
    if big {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            push(emit, base_x + dx, base_y, base_z + dz, VoxelType::Wood);
        }
    }

    // Canopy: ragged ellipsoid centred one block above the trunk
    let cy = base_y + trunk_h;
    let cr = 2.0 + r(0x2_0002) * 1.2; // 2.0..3.2 blocks
    let (rx, ry, rz) = (cr, cr * 0.75, cr);
    let (irx, iry, irz) = (rx.ceil() as i32, ry.ceil() as i32, rz.ceil() as i32);
    for dy in -iry..=iry {
        for dx in -irx..=irx {
            for dz in -irz..=irz {
                let ex = dx as f32 / rx;
                let ey = dy as f32 / ry;
                let ez = dz as f32 / rz;
                let dist = (ex * ex + ey * ey + ez * ez).sqrt();
                // Ragged edge: a per-block hash pushes the surface in/out
                let jitter = 0.3 * (hash01(dx * 31 + dz, dy, hash ^ 0x3_0003) - 0.5);
                if dist + jitter <= 1.0 {
                    // Keep the trunk visible below the canopy centre
                    if dx == 0 && dz == 0 && dy < 0 {
                        continue;
                    }
                    push(emit, base_x + dx, cy + dy, base_z + dz, VoxelType::Leaves);
                }
            }
        }
    }
}

/// Palm tree: a curved trunk that leans away from the wind, crowned by
/// drooping fronds. Lives on the beach band.
pub fn palm(base_x: i32, base_y: i32, base_z: i32, hash: u64, emit: &mut StampFn) {
    let r = |salt: u64| hash01(0, 0, hash ^ salt);

    let trunk_h = 6 + (r(0x11) * 4.0) as i32; // 6..9 blocks
    let dir = r(0x22) * TAU; // lean direction
    let lean = 0.10 + r(0x33) * 0.12;
    let (dir_x, dir_z) = (dir.cos(), dir.sin());

    // Curved trunk: the offset accelerates towards the top (t^2), which
    // reads as the classic palm arc.
    let mut top_x = base_x;
    let mut top_z = base_z;
    for y in 0..trunk_h {
        let t = (y as f32 + 1.0) / trunk_h as f32;
        let off = lean * t * t * trunk_h as f32;
        let x = base_x + (dir_x * off).round() as i32;
        let z = base_z + (dir_z * off).round() as i32;
        push(emit, x, base_y + y, z, VoxelType::Wood);
        top_x = x;
        top_z = z;
    }
    let top_y = base_y + trunk_h;

    // Crown tuft
    push(emit, top_x, top_y, top_z, VoxelType::Leaves);

    // Fronds: `n` arms drooping outwards, each 3 blocks long
    let fronds = 5 + (r(0x44) * 3.0) as i32; // 5..7
    let twist = r(0x55) * TAU;
    for i in 0..fronds {
        let a = (i as f32 / fronds as f32) * TAU + twist;
        let (ax, az) = (a.cos(), a.sin());
        for step in 1..=3 {
            let x = top_x + (ax * step as f32).round() as i32;
            let z = top_z + (az * step as f32).round() as i32;
            // The tip of each frond drops one block
            let y = if step == 3 { top_y - 1 } else { top_y };
            push(emit, x, y, z, VoxelType::Leaves);
        }
    }
}

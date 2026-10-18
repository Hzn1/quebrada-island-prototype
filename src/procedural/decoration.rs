//! Lightweight, deterministic decoration scatter ("spawns").
//!
//! The island surface is divided into a grid of cells with a single
//! candidate per cell. The candidate gets a jittered position (kept
//! `MARGIN` blocks from the cell borders) and a few independent hash
//! rolls that decide *whether* and *what* to spawn.
//!
//! Properties that fall out of this design:
//! - **Non-colliding**: at most one decoration per cell and the jitter is
//!   margin-constrained, so two decorations can never overlap.
//! - **Evenly distributed**: the grid acts like cheap blue-noise sampling
//!   (no clumping, no visible regularity thanks to the jitter).
//! - **Deterministic**: pure hashes, so every chunk computes the exact
//!   same decorations for the cells it shares with its neighbours.

use crate::config;
use crate::procedural::biome::HIGHLAND_LEVEL;
use crate::procedural::noise::{hash01, hash2};
use crate::procedural::terrain::Terrain;
use crate::procedural::tree;
use crate::procedural::voxel_block::{VoxelStamp, VoxelType};

/// Keep decorations at least this many blocks away from the cell border
/// so a canopy can only overhang by ~1 block (chunks account for that
/// with a one-cell generation margin).
const MARGIN: f32 = 2.0;

/// What can spawn at a candidate position.
pub enum Decoration {
    Tree { hash: u64 },
    Palm { hash: u64 },
    Rock { hash: u64 },
    Bush { hash: u64 },
}

/// Evaluates the single candidate of cell `(cell_x, cell_z)`.
///
/// Returns the decoration plus the exact (fractional) world position of
/// its base, or `None` when the cell stays empty (the common case).
pub fn candidate(
    cell_x: i32,
    cell_z: i32,
    terrain: &Terrain,
    seed: u64,
) -> Option<(Decoration, f32, f32)> {
    let cell = config::DECOR_CELL_SIZE as f32;

    // Jittered position inside the cell
    let jx = hash01(cell_x, cell_z, seed ^ 0xA5A5_0001);
    let jz = hash01(cell_x, cell_z, seed ^ 0x5A5A_0002);
    let x = cell_x as f32 * cell + MARGIN + jx * (cell - 2.0 * MARGIN);
    let z = cell_z as f32 * cell + MARGIN + jz * (cell - 2.0 * MARGIN);

    // Environmental readout of the candidate spot
    let height = terrain.get_height_at(x, z);
    if height < config::SEA_LEVEL + 0.5 {
        return None; // submerged - nothing grows under water (yet)
    }
    let slope = terrain.slope_at(x, z);
    let humidity = terrain.humidity_at(x, z);
    let beach_top = config::SEA_LEVEL + config::BEACH_WIDTH;

    // Independent rolls per decision so the checks don't bias each other
    let r_palm = hash01(cell_x, cell_z, seed ^ 0x0004_0004);
    let r_rock = hash01(cell_x, cell_z, seed ^ 0x0002_0002);
    let r_tree = hash01(cell_x, cell_z, seed ^ 0x0001_0001);
    let r_bush = hash01(cell_x, cell_z, seed ^ 0x0003_0003);
    let hash = hash2(cell_x, cell_z, seed ^ 0xDECA_D00D);

    // 1) Palms: the sand strip, flat spots only, sparse
    if height <= beach_top + 2.0 && slope <= 1.5 && r_palm < 0.22 {
        return Some((Decoration::Palm { hash }, x, z));
    }

    // 2) Rocks: anywhere above water; denser on steep and high ground
    let rock_density = config::ROCK_DENSITY
        * (1.0 + 0.25 * slope)
        * if height >= HIGHLAND_LEVEL { 2.5 } else { 1.0 };
    if r_rock < rock_density {
        return Some((Decoration::Rock { hash }, x, z));
    }

    // 3) Trees: only on soil biomes, flat enough, not above the highlands;
    //    density scales with humidity -> humid cells become forest
    if slope <= 2.0 && height < HIGHLAND_LEVEL && humidity > 0.3 {
        let tree_density = config::TREE_DENSITY * (0.4 + 1.2 * humidity);
        if r_tree < tree_density {
            return Some((Decoration::Tree { hash }, x, z));
        }
    }

    // 4) Bushes: cheap filler vegetation on grass
    if slope <= 2.0 && height < HIGHLAND_LEVEL && r_bush < 0.10 {
        return Some((Decoration::Bush { hash }, x, z));
    }

    None
}

/// Emits all voxel stamps of a decoration through `emit`.
/// `base_y` is the quantized surface height of the candidate position.
pub fn stamp(dec: &Decoration, base_x: i32, base_y: i32, base_z: i32, emit: &mut tree::StampFn) {
    match dec {
        Decoration::Tree { hash } => tree::tree(base_x, base_y, base_z, *hash, emit),
        Decoration::Palm { hash } => tree::palm(base_x, base_y, base_z, *hash, emit),
        Decoration::Rock { hash } => rock(base_x, base_y, base_z, *hash, emit),
        Decoration::Bush { hash } => bush(base_x, base_y, base_z, *hash, emit),
    }
}

/// Rock formation: a squashed, ragged ellipsoid partially sunk into the
/// ground (its bottom blocks are buried and get culled by the mesher).
fn rock(base_x: i32, base_y: i32, base_z: i32, hash: u64, emit: &mut tree::StampFn) {
    let r = |salt: u64| hash01(0, 0, hash ^ salt);

    let radius = 1.4 + r(0x66) * 1.6; // 1.4..3.0 blocks wide
    let ry = radius * (0.55 + r(0x77) * 0.4); // squashed vertically
    let ir = radius.ceil() as i32;
    for dy in -1..=ir {
        // dy = -1 sinks the base one block into the ground
        for dx in -ir..=ir {
            for dz in -ir..=ir {
                let ex = dx as f32 / radius;
                let ey = dy as f32 / ry;
                let ez = dz as f32 / radius;
                let dist = (ex * ex + ey * ey + ez * ez).sqrt();
                let jitter = 0.25 * (hash01(dx * 17 + dz, dy, hash ^ 0x88) - 0.5);
                if dist + jitter <= 1.0 {
                    emit(VoxelStamp::new(base_x + dx, base_y + dy, base_z + dz, VoxelType::Stone));
                }
            }
        }
    }
}

/// Bush: a small ellipsoid of leaves hugging the ground.
fn bush(base_x: i32, base_y: i32, base_z: i32, hash: u64, emit: &mut tree::StampFn) {
    let r = |salt: u64| hash01(0, 0, hash ^ salt);

    let radius = 0.9 + r(0x99) * 0.8; // 0.9..1.7 blocks wide
    let ry = radius * 0.8;
    let cy = ry * 0.7; // centre sits slightly above the ground
    for dy in 0..=2 {
        for dx in -2..=2 {
            for dz in -2..=2 {
                let ex = dx as f32 / radius;
                let ey = (dy as f32 - cy) / ry;
                let ez = dz as f32 / radius;
                if ex * ex + ey * ey + ez * ez <= 1.0 {
                    emit(VoxelStamp::new(base_x + dx, base_y + dy, base_z + dz, VoxelType::Leaves));
                }
            }
        }
    }
}

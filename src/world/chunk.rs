//! Chunk: a `CHUNK_SIZE x CHUNK_SIZE` slice of the island with a render
//! mesh built **once**, when the chunk enters the render radius. After
//! that, drawing a chunk costs a single `draw_mesh` call - no per-frame
//! CPU work, no per-block allocations.
//!
//! The mesher is a heightmap mesher: it only emits faces that can actually
//! be seen (top faces + sides facing a lower neighbour), with run-length
//! encoding so a tall cliff face is a handful of quads instead of one per
//! block. That keeps the vertex count ~6x lower than naive `draw_cube`.

use std::collections::HashMap;

use macroquad::prelude::*;

use crate::config;
use crate::procedural::biome::Biome;
use crate::procedural::decoration;
use crate::procedural::noise::hash01;
use crate::procedural::terrain::Terrain;
use crate::procedural::voxel_block::{VoxelBlock, VoxelStamp, VoxelType};

/// Directional face lighting (Minecraft-style): flat colored faces read
/// much better than uniform lighting on a low-poly voxel look.
const LIGHT_TOP: f32 = 1.0;
const LIGHT_SIDE_Z: f32 = 0.82; // faces with a Z normal
const LIGHT_SIDE_X: f32 = 0.68; // faces with an X normal
const LIGHT_BOTTOM: f32 = 0.5;

/// Side length of a chunk in blocks (re-exported from the config).
pub const SIZE: i32 = config::CHUNK_SIZE;

/// A prebuilt piece of the world.
pub struct Chunk {
    /// Chunk coordinates: covers `[cx*SIZE, (cx+1)*SIZE)` on X and Z.
    pub cx: i32,
    pub cz: i32,
    /// Static geometry: terrain surface + decoration blocks.
    pub mesh: Mesh,
    /// Average water depth over the submerged columns (`None` = dry chunk).
    /// Used to tint this chunk's sea quads.
    pub water_depth: Option<f32>,
    /// Bounding sphere (world space) for cheap render-time culling.
    pub center: Vec3,
    pub radius: f32,
}

/// Appends one quad (two CCW triangles) to the mesh being built.
fn push_quad(
    verts: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    corners: [Vec3; 4],
    normal: Vec3,
    color: Color,
) {
    let base = verts.len() as u16;
    let n = Vec4::new(normal.x, normal.y, normal.z, 0.0);
    for p in corners {
        let mut v = Vertex::new2(p, Vec2::ZERO, color);
        v.normal = n; // unused by macroquad's default shader, kept for future lighting
        verts.push(v);
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Final vertex color: base color x face light x per-column value jitter
/// (breaks up tiling repetition). Grass gets its dry/wet tint through the
/// `tint` parameter (see `grass_tint`).
fn face_color(kind: VoxelType, jitter: f32, light: f32, tint: Vec3) -> Color {
    let base = VoxelBlock::get_color_for_type(kind);
    let m = light * jitter;
    let r = (base.r * tint.x * m * 255.0).clamp(0.0, 255.0) as u8;
    let g = (base.g * tint.y * m * 255.0).clamp(0.0, 255.0) as u8;
    let b = (base.b * tint.z * m * 255.0).clamp(0.0, 255.0) as u8;
    Color::from_rgba(r, g, b, 255)
}

/// Humidity tint applied to grass (and only to grass).
fn grass_tint(humidity: f32) -> Vec3 {
    Vec3::new(
        lerp(1.20, 0.78, humidity), // dry = yellower, wet = deeper green
        lerp(1.04, 1.00, humidity),
        lerp(0.50, 0.82, humidity),
    )
}

/// Builds the chunk at `(cx, cz)`: column data -> surface mesh ->
/// decoration blocks -> sea depth. Pure and deterministic, so a chunk
/// border is seamless by construction.
pub fn build(cx: i32, cz: i32, terrain: &Terrain, seed: u64) -> Chunk {
    const S: i32 = config::CHUNK_SIZE;
    const B: usize = (config::CHUNK_SIZE + 2) as usize; // 1-block border
    let ox = cx * S;
    let oz = cz * S;

    // --- 1. Column data -------------------------------------------------
    // Heights/humidity are sampled with a one-block border so neighbour
    // lookups at the chunk edge use the very same pure functions the
    // neighbouring chunk uses -> no seams, no cross-chunk dependencies.
    let mut heights = vec![0f32; B * B];
    let mut humids = vec![0f32; B * B];
    for lz in 0..B {
        for lx in 0..B {
            let wx = ox + lx as i32 - 1;
            let wz = oz + lz as i32 - 1;
            let fx = wx as f32 + 0.5;
            let fz = wz as f32 + 0.5;
            heights[lx + lz * B] = terrain.get_height_at(fx, fz);
            humids[lx + lz * B] = terrain.humidity_at(fx, fz);
        }
    }
    let grid_h = |lx: i32, lz: i32| heights[(lx + 1) as usize + (lz + 1) as usize * B];
    // Terrain height at any world column, using the grid when possible.
    let top_at = |x: i32, z: i32| -> i32 {
        if x >= ox && x < ox + S && z >= oz && z < oz + S {
            grid_h(x - ox, z - oz) as i32
        } else {
            terrain.get_height_at(x as f32 + 0.5, z as f32 + 0.5) as i32
        }
    };

    let mut verts: Vec<Vertex> = Vec::with_capacity((S * S * 8) as usize);
    let mut indices: Vec<u16> = Vec::with_capacity((S * S * 12) as usize);

    // --- 2. Surface mesh ------------------------------------------------
    for lz in 0..S {
        for lx in 0..S {
            let wx = ox + lx;
            let wz = oz + lz;
            let top = grid_h(lx, lz) as i32; // heights are already rounded
            let humidity = humids[(lx + 1) as usize + (lz + 1) as usize * B];

            // Slope from the grid (cheap: no extra noise evaluations)
            let slope = (grid_h(lx - 1, lz) - top as f32)
                .abs()
                .max((grid_h(lx + 1, lz) - top as f32).abs())
                .max((grid_h(lx, lz - 1) - top as f32).abs())
                .max((grid_h(lx, lz + 1) - top as f32).abs());

            let biome = Biome::classify(top as f32, humidity, slope);
            let surface = biome.surface_block(hash01(wx, wz, seed ^ 0x0A11_0001));
            let filler = biome.filler_block();
            let top_depth = biome.top_depth();
            // Material of the block at depth y inside this column
            let kind_at = |y: i32| if top - y <= top_depth { surface } else { filler };

            // Visual variation per column
            let jitter = 0.90 + 0.20 * hash01(wx, wz, seed ^ 0xC011_0002);
            let tint = if surface == VoxelType::Grass {
                grass_tint(humidity)
            } else {
                Vec3::ONE
            };

            let w0 = wx as f32;
            let w1 = w0 + 1.0;
            let z0 = wz as f32;
            let z1 = z0 + 1.0;
            let y_top = top as f32;

            // Top face: the walkable surface, at y = top
            push_quad(
                &mut verts,
                &mut indices,
                [vec3(w0, y_top, z0), vec3(w0, y_top, z1), vec3(w1, y_top, z1), vec3(w1, y_top, z0)],
                Vec3::Y,
                face_color(surface, jitter, LIGHT_TOP, tint),
            );

            // Side faces toward lower neighbours, run-length encoded so
            // each material band becomes a single tall quad.
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nh_f = grid_h(lx + dx, lz + dz);
                // Forçar emissão da face lateral caso haja qualquer diferença
                // entre coluna e vizinho (evita buracos entre chunks com
                // arredondamento/numérico). Só não emite se vizinho for >= top.
                if nh_f + 1e-3 >= (top as f32) {
                    continue;
                }
                let (light, normal) = match (dx, dz) {
                    (1, 0) => (LIGHT_SIDE_X, Vec3::X),
                    (-1, 0) => (LIGHT_SIDE_X, Vec3::NEG_X),
                    (0, 1) => (LIGHT_SIDE_Z, Vec3::Z),
                    _ => (LIGHT_SIDE_Z, Vec3::NEG_Z),
                };
                let mut y = nh_f.floor() as i32;
                if y >= top {
                    continue;
                }
                let mut sy = y;
                while sy < top {
                    let kind = kind_at(sy);
                    let mut y2 = sy + 1;
                    while y2 < top && kind_at(y2) == kind {
                        y2 += 1;
                    }
                    if (y2 as f32) < nh_f + 1e-4 {
                        y2 = (nh_f + 1e-4).ceil() as i32;
                    }
                    if y2 > top {
                        y2 = top;
                    }
                    if y2 <= sy {
                        sy += 1;
                        if sy >= top {
                            break;
                        }
                        continue;
                    }
                    let (fy0, fy1) = (sy as f32, y2 as f32);
                    let color = face_color(kind, jitter, light, Vec3::ONE);
                    let corners = match (dx, dz) {
                        (1, 0) => [
                            vec3(w1, fy0, z0), vec3(w1, fy1, z0),
                            vec3(w1, fy1, z1), vec3(w1, fy0, z1),
                        ],
                        (-1, 0) => [
                            vec3(w0, fy0, z1), vec3(w0, fy1, z1),
                            vec3(w0, fy1, z0), vec3(w0, fy0, z0),
                        ],
                        (0, 1) => [
                            vec3(w0, fy0, z1), vec3(w1, fy0, z1),
                            vec3(w1, fy1, z1), vec3(w0, fy1, z1),
                        ],
                        _ => [
                            vec3(w1, fy0, z0), vec3(w0, fy0, z0),
                            vec3(w0, fy1, z0), vec3(w1, fy1, z0),
                        ],
                    };
                    push_quad(&mut verts, &mut indices, corners, normal, color);
                    sy = y2;
                }
            }
        }
    }

    // --- 3. Decorations (trees, palms, rocks, bushes) -------------------
    // Candidate cells overlapping this chunk plus a one-cell margin: a
    // canopy that overhangs the border is emitted by both chunks, and each
    // one keeps only the blocks inside its own bounds -> no gaps, no
    // double geometry, no cross-chunk state.
    let cell = config::DECOR_CELL_SIZE;
    let cell_min_x = ox.div_euclid(cell) - 1;
    let cell_max_x = (ox + S - 1).div_euclid(cell) + 1;
    let cell_min_z = oz.div_euclid(cell) - 1;
    let cell_max_z = (oz + S - 1).div_euclid(cell) + 1;

    let mut dec_blocks: HashMap<(i32, i32, i32), VoxelType> = HashMap::new();
    for ccz in cell_min_z..=cell_max_z {
        for ccx in cell_min_x..=cell_max_x {
            let Some((dec, fx, fz)) = decoration::candidate(ccx, ccz, terrain, seed) else {
                continue;
            };
            let bx = fx.floor() as i32;
            let bz = fz.floor() as i32;
            let by = terrain.get_height_at(fx, fz) as i32; // ground surface
            decoration::stamp(&dec, bx, by, bz, &mut |s: VoxelStamp| {
                if s.x >= ox && s.x < ox + S && s.z >= oz && s.z < oz + S {
                    dec_blocks.insert((s.x, s.y, s.z), s.kind);
                }
            });
        }
    }

    // Mesh the decoration blocks, occluded by the terrain and by each other
    for (&(x, y, z), &kind) in &dec_blocks {
        let ground_top = top_at(x, z);
        if y < ground_top {
            continue; // fully buried in terrain - never visible
        }
        let jitter = 0.88 + 0.24 * hash01(x, z + y * 131, seed ^ 0x0DEC_0003);
        // Leaves get a per-plant hue shift so a forest isn't monochrome
        let tint = if kind == VoxelType::Leaves {
            let r = hash01(x, z, seed ^ 0x1EAF_0004);
            Vec3::new(0.80 + 0.40 * r, 0.90 + 0.20 * r, 0.65 + 0.35 * r)
        } else {
            Vec3::ONE
        };
        let (w0, w1, z0, z1) = (x as f32, x as f32 + 1.0, z as f32, z as f32 + 1.0);
        let (fy0, fy1) = (y as f32, y as f32 + 1.0);

        // Top face
        if !dec_blocks.contains_key(&(x, y + 1, z)) {
            push_quad(
                &mut verts,
                &mut indices,
                [vec3(w0, fy1, z0), vec3(w0, fy1, z1), vec3(w1, fy1, z1), vec3(w1, fy1, z0)],
                Vec3::Y,
                face_color(kind, jitter, LIGHT_TOP, tint),
            );
        }
        // Side faces
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            if top_at(nx, nz) <= y && !dec_blocks.contains_key(&(nx, y, nz)) {
                let (light, normal) = match (dx, dz) {
                    (1, 0) => (LIGHT_SIDE_X, Vec3::X),
                    (-1, 0) => (LIGHT_SIDE_X, Vec3::NEG_X),
                    (0, 1) => (LIGHT_SIDE_Z, Vec3::Z),
                    _ => (LIGHT_SIDE_Z, Vec3::NEG_Z),
                };
                let color = face_color(kind, jitter, light, tint);
                let corners = match (dx, dz) {
                    (1, 0) => [
                        vec3(w1, fy0, z0), vec3(w1, fy1, z0),
                        vec3(w1, fy1, z1), vec3(w1, fy0, z1),
                    ],
                    (-1, 0) => [
                        vec3(w0, fy0, z1), vec3(w0, fy1, z1),
                        vec3(w0, fy1, z0), vec3(w0, fy0, z0),
                    ],
                    (0, 1) => [
                        vec3(w0, fy0, z1), vec3(w1, fy0, z1),
                        vec3(w1, fy1, z1), vec3(w0, fy1, z1),
                    ],
                    _ => [
                        vec3(w1, fy0, z0), vec3(w0, fy0, z0),
                        vec3(w0, fy1, z0), vec3(w1, fy1, z0),
                    ],
                };
                push_quad(&mut verts, &mut indices, corners, normal, color);
            }
        }
        // Bottom face, only for blocks floating above the terrain (fronds,
        // overhangs)
        if y > ground_top && !dec_blocks.contains_key(&(x, y - 1, z)) {
            push_quad(
                &mut verts,
                &mut indices,
                [vec3(w0, fy0, z0), vec3(w1, fy0, z0), vec3(w1, fy0, z1), vec3(w0, fy0, z1)],
                Vec3::NEG_Y,
                face_color(kind, jitter, LIGHT_BOTTOM, tint),
            );
        }
    }

    // The index buffer is u16: a heightmap chunk stays far below the limit
    // (~15k vertices worst case), but fail loudly if that ever changes.
    assert!(
        verts.len() <= u16::MAX as usize,
        "chunk ({cx}, {cz}) mesh has too many vertices for u16 indices"
    );

    // --- 4. Sea depth (per-chunk water tint) ----------------------------
    let mut depth_sum = 0.0f32;
    let mut depth_cols = 0u32;
    for lz in 0..S {
        for lx in 0..S {
            let h = grid_h(lx, lz);
            if h < config::SEA_LEVEL {
                depth_sum += config::SEA_LEVEL - h;
                depth_cols += 1;
            }
        }
    }
    let water_depth = if depth_cols > 0 {
        Some(depth_sum / depth_cols as f32)
    } else {
        None
    };

    let center = vec3(
        ox as f32 + S as f32 * 0.5,
        config::SEA_LEVEL + 12.0,
        oz as f32 + S as f32 * 0.5,
    );
    // Conservative bounding sphere: half the diagonal + tallest peak
    let radius = S as f32 * 0.7071 + 40.0;

    Chunk {
        cx,
        cz,
        mesh: Mesh {
            vertices: verts,
            indices,
            texture: None, // macroquad binds its white texture -> vertex colors
        },
        water_depth,
        center,
        radius,
    }
}

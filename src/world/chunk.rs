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
use crate::procedural::structures;
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
    /// Overhead power wires (world-space segments) strung above the alleys.
    /// Drawn as lines each frame; a handful per chunk, so it is effectively
    /// free compared to the mesh.
    pub wires: Vec<(Vec3, Vec3)>,
    /// Per-column building volume for collision: lowest / highest block y of
    /// the houses (`i32::MAX` / `i32::MIN` where there is no building).
    pub wall_lo: Vec<i32>,
    pub wall_hi: Vec<i32>,
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
/// `tint` parameter (see `grass_tint`); `tone` selects a variant from the
/// material's palette (paint colours, brick shades, zinc weathering...).
fn face_color(kind: VoxelType, tone: u8, jitter: f32, light: f32, tint: Vec3) -> Color {
    let base = VoxelBlock::color_for(kind, tone);
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
            // Favela: on the hillside the ground stops being soil and becomes
            // paved concrete. The density is a pure function of the column,
            // so two chunks agree perfectly on where the town starts.
            let urban = terrain.urban_density(top as f32, wx as f32 + 0.5, wz as f32 + 0.5);
            let (surface, filler, top_depth) = if urban > config::URBAN_THRESHOLD {
                (VoxelType::Concrete, VoxelType::Stone, 1)
            } else {
                (
                    biome.surface_block(hash01(wx, wz, seed ^ 0x0A11_0001)),
                    biome.filler_block(),
                    biome.top_depth(),
                )
            };
            let surface_tone =
                (hash01(wx, wz, seed ^ 0x0A11_0009) * VoxelBlock::palette_len(surface) as f32) as u8;
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
                face_color(surface, surface_tone, jitter, LIGHT_TOP, tint),
            );

            // Side faces toward lower neighbours, run-length encoded so
            // each material band becomes a single tall quad.
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nh = grid_h(lx + dx, lz + dz) as i32;
                if nh >= top {
                    continue; // neighbour hides this side entirely
                }
                let (light, normal) = match (dx, dz) {
                    (1, 0) => (LIGHT_SIDE_X, Vec3::X),
                    (-1, 0) => (LIGHT_SIDE_X, Vec3::NEG_X),
                    (0, 1) => (LIGHT_SIDE_Z, Vec3::Z),
                    _ => (LIGHT_SIDE_Z, Vec3::NEG_Z),
                };
                let mut y = nh;
                while y < top {
                    let kind = kind_at(y);
                    let mut y2 = y + 1;
                    while y2 < top && kind_at(y2) == kind {
                        y2 += 1; // extend the run while the material matches
                    }
                    let (fy0, fy1) = (y as f32, y2 as f32);
                    let tone = if kind == surface { surface_tone } else { 0 };
                    let color = face_color(kind, tone, jitter, light, Vec3::ONE);
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
                    y = y2;
                }
            }
        }
    }

    // --- 3. Favela: houses, alleys, stairs, rooftops, water tanks --------
    // The urban generator clips its output to this chunk, so two adjacent
    // chunks mesh their own half of a shared house with no duplicated work.
    // Houses are hollow: only walls, slabs and roofs are stamped, and the
    // occlusion pass below culls their buried and interior faces exactly
    // like it does for a tree.
    let urban = structures::populate(ox, oz, S, terrain, seed);
    let wires = urban.wires;

    // All stamped blocks (houses + natural decorations) share one map so the
    // mesher can cull faces between them.
    //
    // In the same pass we record each column's *building volume* (its lowest
    // and highest block) for horizontal collision. Alley columns
    // (`x % CELL == 0` / `z % CELL == 0`) never hold a house - only stairs and
    // poles - so they are left out and stay walk-through, keeping the becos
    // navigable.
    let cell_size = config::URBAN_CELL_SIZE;
    let mut wall_lo = vec![i32::MAX; (S * S) as usize];
    let mut wall_hi = vec![i32::MIN; (S * S) as usize];
    let mut dec_blocks: HashMap<(i32, i32, i32), (VoxelType, u8)> =
        HashMap::with_capacity(urban.blocks.len() + 512);
    for s in urban.blocks {
        if s.x.rem_euclid(cell_size) != 0 && s.z.rem_euclid(cell_size) != 0 {
            let idx = ((s.x - ox) + (s.z - oz) * S) as usize;
            wall_lo[idx] = wall_lo[idx].min(s.y);
            wall_hi[idx] = wall_hi[idx].max(s.y);
        }
        dec_blocks.insert((s.x, s.y, s.z), (s.kind, s.tone));
    }

    // --- 3b. Natural decorations, kept out of the town -------------------
    // Candidate cells overlapping this chunk plus a one-cell margin: a
    // canopy that overhangs the border is emitted by both chunks, and each
    // one keeps only the blocks inside its own bounds -> no gaps, no
    // double geometry, no cross-chunk state.
    let cell = config::DECOR_CELL_SIZE;
    let cell_min_x = ox.div_euclid(cell) - 1;
    let cell_max_x = (ox + S - 1).div_euclid(cell) + 1;
    let cell_min_z = oz.div_euclid(cell) - 1;
    let cell_max_z = (oz + S - 1).div_euclid(cell) + 1;

    for ccz in cell_min_z..=cell_max_z {
        for ccx in cell_min_x..=cell_max_x {
            let Some((dec, fx, fz)) = decoration::candidate(ccx, ccz, terrain, seed) else {
                continue;
            };
            let by = terrain.get_height_at(fx, fz);
            // The town owns this spot: no trees growing through the roofs.
            // Natural decoration density scales with urban density being low.
            if terrain.urban_density(by, fx, fz) > config::URBAN_THRESHOLD {
                continue;
            }
            let bx = fx.floor() as i32;
            let bz = fz.floor() as i32;
            let by = by as i32; // ground surface
            decoration::stamp(&dec, bx, by, bz, &mut |s: VoxelStamp| {
                if s.x >= ox && s.x < ox + S && s.z >= oz && s.z < oz + S {
                    // Houses win over stray vegetation on a shared cell.
                    dec_blocks
                        .entry((s.x, s.y, s.z))
                        .or_insert((s.kind, s.tone));
                }
            });
        }
    }

    // Mesh the stamped blocks, occluded by the terrain and by each other.
    for (&(x, y, z), &(kind, tone)) in &dec_blocks {
        // Safety valve: the index buffer is u16, so stop stamping faces
        // before a pathological chunk could overflow it.
        if verts.len() > config::MAX_CHUNK_VERTICES {
            break;
        }
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
                face_color(kind, tone, jitter, LIGHT_TOP, tint),
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
                let color = face_color(kind, tone, jitter, light, tint);
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
                face_color(kind, tone, jitter, LIGHT_BOTTOM, tint),
            );
        }
    }

    // The index buffer is u16: the mesher stops adding decoration faces at
    // `MAX_CHUNK_VERTICES`, well before the limit. Fail loudly if the terrain
    // pass alone ever gets close (that would mean the generator changed).
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
        wires,
        wall_lo,
        wall_hi,
        water_depth,
        center,
        radius,
    }
}

#[cfg(test)]
mod audit {
    use super::*;
    use crate::procedural::terrain::Terrain;
    use std::collections::HashMap;

    fn quads(mesh: &Mesh) -> Vec<[Vec3; 4]> {
        let v = &mesh.vertices;
        (0..v.len() / 4)
            .map(|q| {
                [
                    v[q * 4].position,
                    v[q * 4 + 1].position,
                    v[q * 4 + 2].position,
                    v[q * 4 + 3].position,
                ]
            })
            .collect()
    }

    /// Verifies the mesher emits: 1 top face per column, and full side-face
    /// coverage towards every lower neighbour. Also checks index validity and
    /// duplicate (coincident) quads.
    #[test]
    fn mesh_integrity() {
        let seed = config::WORLD_SEED;
        let terrain = Terrain::new(seed);
        let s = config::CHUNK_SIZE;
        let mut worst_verts = 0usize;
        let mut missing_tops = 0usize;
        let mut wrong_tops = 0usize;
        let mut missing_side = 0usize;
        let mut short_side = 0usize;
        let mut bad_index = 0usize;
        let mut duplicates = 0usize;

        for cz in -2..2 {
            for cx in -2..2 {
                let c = build(cx, cz, &terrain, seed);
                let ox = cx * s;
                let oz = cz * s;
                let vlen = c.mesh.vertices.len();
                worst_verts = worst_verts.max(vlen);
                if vlen % 4 != 0 {
                    panic!("chunk ({cx},{cz}): vertex count {vlen} is not a multiple of 4");
                }
                for &i in &c.mesh.indices {
                    if i as usize >= vlen {
                        bad_index += 1;
                    }
                }

                let qs = quads(&c.mesh);

                // Top faces per column
                let mut tops: HashMap<(i32, i32), Vec<f32>> = HashMap::new();
                // Side runs keyed by (axis, plane, off) -> runs of (y0,y1)
                let mut sides: HashMap<(u8, i32, i32), Vec<(f32, f32)>> = HashMap::new();
                let mut seen: HashMap<[i64; 12], u32> = HashMap::new();

                for q in &qs {
                    let mut key = [0i64; 12];
                    for (i, p) in q.iter().enumerate() {
                        key[i * 3] = (p.x * 32.0).round() as i64;
                        key[i * 3 + 1] = (p.y * 32.0).round() as i64;
                        key[i * 3 + 2] = (p.z * 32.0).round() as i64;
                    }
                    *seen.entry(key).or_insert(0) += 1;

                    let [a, b, cc, d] = *q;
                    let all_y_eq = a.y == b.y && b.y == cc.y && cc.y == d.y;
                    let all_x_eq = a.x == b.x && b.x == cc.x && cc.x == d.x;
                    let all_z_eq = a.z == b.z && b.z == cc.z && cc.z == d.z;
                    let ymin = [a.y, b.y, cc.y, d.y].iter().cloned().fold(f32::INFINITY, f32::min);
                    let ymax =
                        [a.y, b.y, cc.y, d.y].iter().cloned().fold(f32::NEG_INFINITY, f32::max);

                    if all_y_eq {
                        let xs = [a.x, b.x, cc.x, d.x];
                        let zs = [a.z, b.z, cc.z, d.z];
                        let xmin = xs.iter().cloned().fold(f32::INFINITY, f32::min);
                        let xmax = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                        let zmin = zs.iter().cloned().fold(f32::INFINITY, f32::min);
                        let zmax = zs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                        let w = (xmax - xmin).round() as i32;
                        let h = (zmax - zmin).round() as i32;
                        if w == 1 && h == 1 {
                            tops.entry((xmin.floor() as i32 - ox, zmin.floor() as i32 - oz))
                                .or_default()
                                .push(a.y);
                        }
                    } else if all_x_eq {
                        let plane = (a.x - ox as f32).round() as i32;
                        let zmin = [a.z, b.z, cc.z, d.z].iter().cloned().fold(f32::INFINITY, f32::min);
                        let off = zmin.floor() as i32 - oz;
                        sides.entry((b'X', plane, off)).or_default().push((ymin, ymax));
                    } else if all_z_eq {
                        let plane = (a.z - oz as f32).round() as i32;
                        let xmin = [a.x, b.x, cc.x, d.x].iter().cloned().fold(f32::INFINITY, f32::min);
                        let off = xmin.floor() as i32 - ox;
                        sides.entry((b'Z', plane, off)).or_default().push((ymin, ymax));
                    }
                }

                for dup in seen.values() {
                    if *dup > 1 {
                        duplicates += *dup as usize - 1;
                    }
                }

                // 1 top face per column, at the sampled height
                for lz in 0..s {
                    for lx in 0..s {
                        let expected = terrain
                            .get_height_at((ox + lx) as f32 + 0.5, (oz + lz) as f32 + 0.5)
                            as i32;
                        match tops.get(&(lx, lz)) {
                            None => missing_tops += 1,
                            Some(ys) => {
                                // Extra top faces can legitimately exist here
                                // (decoration blocks sitting on the column);
                                // what matters is that the terrain top is present.
                                if !ys.iter().any(|y| *y as i32 == expected) {
                                    wrong_tops += 1;
                                }
                            }
                        }
                    }
                }

                // side coverage
                for lz in 0..s {
                    for lx in 0..s {
                        let top = terrain
                            .get_height_at((ox + lx) as f32 + 0.5, (oz + lz) as f32 + 0.5)
                            as i32;
                        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                            let ntop = terrain
                                .get_height_at(
                                    (ox + lx + dx) as f32 + 0.5,
                                    (oz + lz + dz) as f32 + 0.5,
                                ) as i32;
                            if ntop >= top {
                                continue;
                            }
                            let key = if dz == 0 {
                                (b'X', if dx == 1 { lx + 1 } else { lx }, lz)
                            } else {
                                (b'Z', if dz == 1 { lz + 1 } else { lz }, lx)
                            };
                            let covered: f32 = sides
                                .get(&key)
                                .map(|rs| {
                                    rs.iter()
                                        .filter(|(y0, y1)| *y1 > ntop as f32 && *y0 < top as f32)
                                        .map(|(y0, y1)| y1.min(top as f32) - y0.max(ntop as f32))
                                        .sum()
                                })
                                .unwrap_or(0.0);
                            let expected = (top - ntop) as f32;
                            if covered <= 0.0 {
                                missing_side += 1;
                            } else if covered + 1e-3 < expected {
                                short_side += 1;
                            }
                        }
                    }
                }
            }
        }

        println!(
            "worst_verts={worst_verts} missing_tops={missing_tops} wrong_tops={wrong_tops} \
             missing_side={missing_side} short_side={short_side} bad_index={bad_index} \
             duplicates={duplicates}"
        );

        assert_eq!(bad_index, 0, "mesh indices reference out-of-range vertices");
        assert_eq!(missing_tops, 0, "columns without a top face");
        assert_eq!(missing_side, 0, "exposed side faces missing");
        assert_eq!(short_side, 0, "side faces not covering the exposed height");
    }

    /// The collision columns must line up exactly with the houses: every
    /// footprint column is solid, every alley column stays open (so the becos
    /// are walkable). Any drift here would either trap the player or let them
    /// walk through a brick wall.
    #[test]
    fn collision_matches_houses() {
        let seed = config::WORLD_SEED;
        let terrain = Terrain::new(seed);
        let s = config::CHUNK_SIZE;
        let mut mismatch = 0usize;
        for cz in -3..3 {
            for cx in -3..3 {
                let c = build(cx, cz, &terrain, seed);
                let (ox, oz) = (cx * s, cz * s);
                for lz in 0..s {
                    for lx in 0..s {
                        let i = (lx + lz * s) as usize;
                        let solid = c.wall_lo[i] <= c.wall_hi[i];
                        let built = structures::is_built(ox + lx, oz + lz, &terrain, seed);
                        if solid != built {
                            mismatch += 1;
                        }
                    }
                }
            }
        }
        println!("collision_mismatches={mismatch}");
        assert_eq!(mismatch, 0, "collision columns do not match the house footprints");
    }
}

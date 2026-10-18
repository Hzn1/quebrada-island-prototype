//! Favela (urban) generator.
//!
//! The hillside is tiled by a **world-aligned grid** of cells
//! (`config::URBAN_CELL_SIZE`). Each cell deterministically decides whether it
//! hosts a house, and houses always leave a strip of the cell open on their
//! -X / -Z edges. The narrow alleys ("becos") are therefore just the columns
//! `x % CELL == 0` / `z % CELL == 0`: a maze of tight streets falls out of the
//! grid with zero extra bookkeeping, and the layout is identical on both sides
//! of a chunk border (every decision is a pure function of `(cell, seed)`).
//!
//! Houses are stacked *up the slope* rather than into towers: the floor sits on
//! the plot's lowest corner, so the uphill part of the building is buried in
//! the hill and gets culled by the mesher - which is exactly the stepped
//! "amontoada" silhouette of a real favela.
//!
//! Everything is emitted as [`VoxelStamp`]s and clipped to the chunk being
//! built, reusing the chunk mesher's existing occlusion pass (it already culls
//! faces against the terrain and against the other decoration blocks).

use macroquad::prelude::*;

use crate::config;
use crate::procedural::noise::hash01;
use crate::procedural::terrain::Terrain;
use crate::procedural::voxel_block::{VoxelStamp, VoxelType};

// Deterministic salts, one per independent decision. Any change reshuffles the
// whole town, so they are grouped here on purpose.
const S_HOUSE: u64 = 0xFA7E_0001;
const S_FLOORS: u64 = 0xFA7E_0002;
const S_TONE: u64 = 0xFA7E_0004;
const S_WALL: u64 = 0xFA7E_0008;
const S_DOOR: u64 = 0xFA7E_0010;
const S_ZINC: u64 = 0xFA7E_0020;
const S_TANK: u64 = 0xFA7E_0040;
const S_TANK_POS: u64 = 0xFA7E_0080;
const S_POLE: u64 = 0xFA7E_0100;
const S_POLE_H: u64 = 0xFA7E_0200;
const S_SHAPE: u64 = 0xFA7E_0400;

/// Urban content produced for one chunk.
pub struct Urban {
    /// Blocks to merge into the chunk's decoration pass.
    pub blocks: Vec<VoxelStamp>,
    /// Overhead wire segments (world space) for the render pass to stroke.
    pub wires: Vec<(Vec3, Vec3)>,
}

/// One floor-plan of a house, resolved from the terrain.
struct Plan {
    x0: i32,
    x1: i32,
    z0: i32,
    z1: i32,
    /// Floor level of the ground storey (the plot's downhill corner).
    base_y: i32,
    floors: i32,
    /// Wall material per storey (0 = ground, ...).
    wall: [VoxelType; 3],
    /// Paint/brick tone per storey.
    tones: [u8; 3],
    /// Window offsets from `x0` / `z0`, derived from the real width so an
    /// irregular (shrunken) lot still gets windows inside its walls.
    win_x: [i32; 2],
    win_z: [i32; 2],
    /// Z of the ground-floor doorway (on the -X street wall).
    door_z: i32,
    /// Roof: `0` bare laje + parapet, `1` raised zinc sheet, `2` terracotta
    /// tiles.
    roof: u8,
    tank: bool,
    stairs: Option<Stairs>,
}

/// A concrete run of steps threading the alley next to a house.
struct Stairs {
    x: i32,
    z0: i32,
    z1: i32,
    y0: i32,
    steps: i32,
}

/// A power pole (alley corner) with its top position.
struct Pole {
    x: i32,
    z: i32,
    base_y: i32,
    top_y: i32,
}

/// Populates the chunk `[ox, ox+size) x [oz, oz+size)` with urban content.
///
/// Cells overlapping the chunk (plus a one-cell margin, because a building can
/// reach into the neighbour) are planned, then every block is clipped to the
/// chunk bounds - so two adjacent chunks mesh their own half of a shared house
/// with no duplicated geometry.
pub fn populate(ox: i32, oz: i32, size: i32, terrain: &Terrain, seed: u64) -> Urban {
    let cell = config::URBAN_CELL_SIZE;
    let c0x = ox.div_euclid(cell) - 1;
    let c1x = (ox + size - 1).div_euclid(cell) + 1;
    let c0z = oz.div_euclid(cell) - 1;
    let c1z = (oz + size - 1).div_euclid(cell) + 1;

    let mut out = Urban {
        blocks: Vec::with_capacity(4096),
        wires: Vec::new(),
    };

    for cz in c0z..=c1z {
        for cx in c0x..=c1x {
            // --- Houses, stairs, rooftops ---------------------------------
            if let Some(plan) = plan_cell(cx, cz, terrain, seed) {
                stamp_plan(&plan, ox, oz, size, seed, &mut out);
            }

            // --- Poles and wiring -----------------------------------------
            // Poles are resolved by `pole_at` alone, and wires connect the
            // poles of neighbouring cells. Both are pure functions of the
            // cell, so a wire is emitted exactly once (by the cell it starts
            // in) and never duplicated across chunk borders.
            if let Some(pole) = pole_at(cx, cz, terrain, seed) {
                stamp_pole(&pole, ox, oz, size, &mut out);
                let top = vec3(pole.x as f32 + 0.5, pole.top_y as f32, pole.z as f32 + 0.5);
                for (nx, nz) in [(cx + 1, cz), (cx, cz + 1)] {
                    if let Some(next) = pole_at(nx, nz, terrain, seed) {
                        let ntop =
                            vec3(next.x as f32 + 0.5, next.top_y as f32, next.z as f32 + 0.5);
                        emit_wire(top, ntop, ox, oz, size, &mut out);
                    }
                }
            }
        }
    }

    out
}

/// True when a house footprint covers `(x, z)`. Used by the spawn finder so
/// the player never starts inside a wall.
pub fn is_built(x: i32, z: i32, terrain: &Terrain, seed: u64) -> bool {
    let cell = config::URBAN_CELL_SIZE;
    let cx = x.div_euclid(cell);
    let cz = z.div_euclid(cell);
    match plan_cell(cx, cz, terrain, seed) {
        Some(p) => x >= p.x0 && x <= p.x1 && z >= p.z0 && z <= p.z1,
        None => false,
    }
}

/// Clips a `VoxelStamp` push to the chunk bounds.
#[inline]
fn inside(x: i32, z: i32, ox: i32, oz: i32, size: i32) -> bool {
    x >= ox && x < ox + size && z >= oz && z < oz + size
}

/// Resolves the house (if any) of one urban cell from the terrain.
fn plan_cell(cx: i32, cz: i32, terrain: &Terrain, seed: u64) -> Option<Plan> {
    let cell = config::URBAN_CELL_SIZE;
    let ox = cx * cell;
    let oz = cz * cell;
    let s = cell as f32;
    let mid_x = ox as f32 + s * 0.5;
    let mid_z = oz as f32 + s * 0.5;

    // Cheap rejection first: most cells (ocean, beach, countryside) are
    // settled with a single height sample instead of five.
    let centre_h = terrain.get_height_at(mid_x, mid_z);
    if centre_h < config::SEA_LEVEL + 1.5 {
        return None;
    }
    let density = terrain.urban_density(centre_h, mid_x, mid_z);
    if density < config::URBAN_THRESHOLD {
        return None;
    }
    // Density doubles as the probability that a cell actually gets a house,
    // which is what breaks the grid into clusters and empty lots.
    if hash01(cx, cz, seed ^ S_HOUSE) > density {
        return None;
    }

    // Terrain read at the four corners: the low corner sets the floor level,
    // the spread tells us how steep the plot is.
    let mut hmin = centre_h;
    let mut hmax = centre_h;
    let corners = [
        (ox as f32 + 0.5, oz as f32 + 0.5),
        (ox as f32 + s - 0.5, oz as f32 + 0.5),
        (ox as f32 + 0.5, oz as f32 + s - 0.5),
        (ox as f32 + s - 0.5, oz as f32 + s - 0.5),
    ];
    for &(fx, fz) in &corners {
        let h = terrain.get_height_at(fx, fz);
        hmin = hmin.min(h);
        hmax = hmax.max(h);
    }

    // Nothing is built on the beach/underwater, and sheer cliffs are left to
    // the staircases instead of holding a house.
    if hmin < config::SEA_LEVEL + 1.0 {
        return None;
    }
    let relief = (hmax - hmin).round() as i32;
    if relief > 14 {
        return None;
    }

    let base_y = hmin.round() as i32;

    // Storeys: taller where the town is denser, plus a little noise.
    let mut floors = 1;
    if density > 0.62 {
        floors += 1;
    }
    if density > 0.82 {
        floors += 1;
    }
    if hash01(cx, cz, seed ^ S_FLOORS) < 0.28 {
        floors += 1;
    }
    let floors = floors.min(config::MAX_FLOORS);

    // Footprint: the cell minus its alley strips. Occasionally a lot is one
    // block smaller on a side (widening the alley there), which breaks the
    // grid's regularity so the town reads as grown, not stamped.
    let x0 = ox + config::URBAN_ALLEY;
    let z0 = oz + config::URBAN_ALLEY;
    let shrink_x = (hash01(cx, cz, seed ^ S_SHAPE) < 0.40) as i32;
    let shrink_z = (hash01(cx, cz, seed ^ (S_SHAPE + 1)) < 0.40) as i32;
    let x1 = ox + cell - 1 - shrink_x;
    let z1 = oz + cell - 1 - shrink_z;
    let (w, d) = (x1 - x0 + 1, z1 - z0 + 1); // 6 or 7 blocks each
    // Two windows per street wall, always inside the walls.
    let win_x = [2, (w - 2).max(3)];
    let win_z = [2, (d - 2).max(3)];

    // Per-storey material: ground floors lean exposed brick, the upper ones
    // are more often painted (the classic "puxadinho" extension).
    let mut wall = [VoxelType::Brick; 3];
    let mut tones = [0u8; 3];
    for f in 0..3usize {
        let brick_bias = if f == 0 { 0.55 } else { 0.30 };
        let r = hash01(cx, cz, seed ^ (S_WALL + f as u64 * 7));
        wall[f] = if r < brick_bias {
            VoxelType::Brick
        } else {
            VoxelType::Wall
        };
        let tone_r = hash01(cx, cz, seed ^ (S_TONE + f as u64 * 13));
        tones[f] = (tone_r * 255.0) as u8;
    }

    // Doorway on the -X street wall, kept clear of the corners.
    let door_span = (d - 4).max(1) as f32;
    let door_z = z0 + 2 + (hash01(cx, cz, seed ^ S_DOOR) * door_span) as i32;
    let zinc = hash01(cx, cz, seed ^ S_ZINC);
    let roof = if zinc < config::ZINC_ROOF_CHANCE {
        1 // raised zinc sheet
    } else if zinc < config::ZINC_ROOF_CHANCE + config::TILE_ROOF_CHANCE {
        2 // terracotta tiles
    } else {
        0 // bare laje with a parapet
    };

    // Water tanks go on the higher roofs: the ones that read against the sky.
    let roof_y = base_y + floors * config::FLOOR_HEIGHT;
    let tank = floors >= 2
        && hash01(cx, cz, seed ^ S_TANK) < config::WATER_TANK_CHANCE
        && is_local_high(cx, cz, roof_y, terrain);

    // Steep plots get a concrete staircase in the alley.
    let stairs = if relief >= 3 {
        Some(Stairs {
            x: ox,
            z0: z0 + 1,
            z1,
            y0: base_y,
            steps: relief.min(z1 - z0 - 1).max(1),
        })
    } else {
        None
    };

    Some(Plan {
        x0,
        x1,
        z0,
        z1,
        base_y,
        floors,
        wall,
        tones,
        win_x,
        win_z,
        door_z,
        roof,
        tank,
        stairs,
    })
}

/// True when a roof at `roof_y` stands clear of the surrounding ground, i.e.
/// the house reads against the sky and its tank is worth the geometry.
fn is_local_high(cx: i32, cz: i32, roof_y: i32, terrain: &Terrain) -> bool {
    let cell = config::URBAN_CELL_SIZE;
    let s = cell as f32;
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let nx = ((cx + dx) * cell) as f32 + s * 0.5;
        let nz = ((cz + dz) * cell) as f32 + s * 0.5;
        if terrain.get_height_at(nx, nz).round() as i32 >= roof_y {
            return false;
        }
    }
    true
}

/// Emits every voxel of one house (clipped to the chunk).
fn stamp_plan(p: &Plan, ox: i32, oz: i32, size: i32, seed: u64, out: &mut Urban) {
    let fh = config::FLOOR_HEIGHT;
    let mut put = |x: i32, y: i32, z: i32, kind: VoxelType, tone: u8| {
        if inside(x, z, ox, oz, size) {
            out.blocks.push(VoxelStamp { x, y, z, kind, tone });
        }
    };

    for f in 0..p.floors {
        let y0 = p.base_y + f * fh;
        let wall = p.wall[f as usize];
        let tone = p.tones[f as usize];
        let wy = y0 + 1; // first wall row above the slab
        let wall_top = y0 + fh; // exclusive
        // Windows sit on the upper wall row of each storey.
        let win_y = wall_top - 1;

        // --- Floor slab (laje) ------------------------------------------
        for x in p.x0..=p.x1 {
            for z in p.z0..=p.z1 {
                put(x, y0, z, VoxelType::Concrete, 1);
            }
        }

        // --- Walls ------------------------------------------------------
        for y in wy..wall_top {
            // Walls running along X (at z0 and z1).
            for x in p.x0..=p.x1 {
                let kind = if y == win_y && (x == p.x0 + p.win_x[0] || x == p.x0 + p.win_x[1]) {
                    VoxelType::Glass
                } else {
                    wall
                };
                put(x, y, p.z0, kind, tone);
                put(x, y, p.z1, wall, tone);
            }
            // Walls running along Z (at x0 and x1). The ground-floor doorway
            // is left open on the x0 side.
            for z in p.z0 + 1..p.z1 {
                let doorway = f == 0 && z == p.door_z && (y == y0 + 1 || y == y0 + 2);
                if !doorway {
                    let kind = if y == win_y && (z == p.z0 + p.win_z[0] || z == p.z0 + p.win_z[1]) {
                        VoxelType::Glass
                    } else {
                        wall
                    };
                    put(p.x0, y, z, kind, tone);
                }
                put(p.x1, y, z, wall, tone);
            }
        }
    }

    // --- Roof -----------------------------------------------------------
    let ry = p.base_y + p.floors * fh;
    for x in p.x0..=p.x1 {
        for z in p.z0..=p.z1 {
            put(x, ry, z, VoxelType::Concrete, 0);
        }
    }

    // Roof covering: a raised zinc / terracotta sheet, or a bare laje with a
    // low parapet (the classic "laje de lazer").
    let roof_surface;
    if p.roof != 0 {
        let sheet = if p.roof == 1 {
            VoxelType::Zinc
        } else {
            VoxelType::Tile
        };
        for x in p.x0..=p.x1 {
            for z in p.z0..=p.z1 {
                put(x, ry + 1, z, sheet, 0);
            }
        }
        roof_surface = ry + 1;
    } else {
        for x in p.x0..=p.x1 {
            put(x, ry + 1, p.z0, VoxelType::Concrete, 2);
            put(x, ry + 1, p.z1, VoxelType::Concrete, 2);
        }
        for z in p.z0 + 1..p.z1 {
            put(p.x0, ry + 1, z, VoxelType::Concrete, 2);
            put(p.x1, ry + 1, z, VoxelType::Concrete, 2);
        }
        roof_surface = ry;
    }

    // --- Rooftop water tank ---------------------------------------------
    if p.tank {
        // 2x2 footprint anchored on one corner of the roof.
        let canned_x = if hash01(p.x0, p.z0, seed ^ S_TANK_POS) < 0.5 {
            p.x0 + 1
        } else {
            p.x1 - 2
        };
        let canned_z = if hash01(p.z0, p.x0, seed ^ S_TANK_POS) < 0.5 {
            p.z0 + 1
        } else {
            p.z1 - 2
        };
        let base = roof_surface + 1;
        for dx in 0..2 {
            for dz in 0..2 {
                put(canned_x + dx, base, canned_z + dz, VoxelType::BluePlastic, 0);
                put(canned_x + dx, base + 1, canned_z + dz, VoxelType::BluePlastic, 0);
                put(canned_x + dx, base + 2, canned_z + dz, VoxelType::BluePlastic, 0);
            }
        }
    }

    // --- Alley staircase -------------------------------------------------
    if let Some(st) = &p.stairs {
        stamp_stairs(st, ox, oz, size, out);
    }
}

/// A rising run of concrete steps inside the alley column.
fn stamp_stairs(st: &Stairs, ox: i32, oz: i32, size: i32, out: &mut Urban) {
    let mut put = |x: i32, y: i32, z: i32| {
        if inside(x, z, ox, oz, size) {
            out.blocks.push(VoxelStamp {
                x,
                y,
                z,
                kind: VoxelType::Concrete,
                tone: 1,
            });
        }
    };
    let steps = st.steps.max(1);
    let span = (st.z1 - st.z0).max(1);
    for i in 0..steps {
        let z = st.z0 + (i * span) / steps;
        let y = st.y0 + i;
        put(st.x, y, z); // tread
        put(st.x, y, z + 1); // tread is 2 blocks deep
        put(st.x, y - 1, z + 1); // riser
    }
}

/// Resolves the power pole of one alley corner (`cell * CELL` on both axes).
fn pole_at(cx: i32, cz: i32, terrain: &Terrain, seed: u64) -> Option<Pole> {
    let cell = config::URBAN_CELL_SIZE;
    let px = cx * cell;
    let pz = cz * cell;
    let gx = px as f32 + 0.5;
    let gz = pz as f32 + 0.5;
    let gy = terrain.get_height_at(gx, gz);
    if gy < config::SEA_LEVEL + 1.0 {
        return None;
    }
    // Poles only stand where the town is (cheap single noise lookup).
    if terrain.urban_density(gy, gx, gz) < 0.40 {
        return None;
    }
    if hash01(cx, cz, seed ^ S_POLE) >= config::POLE_CHANCE {
        return None;
    }
    let base_y = gy.round() as i32;
    let height = 5 + (hash01(cx, cz, seed ^ S_POLE_H) * 3.0) as i32;
    Some(Pole {
        x: px,
        z: pz,
        base_y,
        top_y: base_y + height,
    })
}

/// Emits the pole column itself.
fn stamp_pole(pole: &Pole, ox: i32, oz: i32, size: i32, out: &mut Urban) {
    if !inside(pole.x, pole.z, ox, oz, size) {
        return;
    }
    for y in pole.base_y..pole.top_y {
        out.blocks.push(VoxelStamp {
            x: pole.x,
            y,
            z: pole.z,
            kind: VoxelType::Wood,
            tone: 0,
        });
    }
    // A short cross-arm at the top gives the pole its characteristic silhouette.
    out.blocks.push(VoxelStamp {
        x: pole.x + 1,
        y: pole.top_y - 1,
        z: pole.z,
        kind: VoxelType::Wood,
        tone: 0,
    });
}

/// Emits a three-segment wire with a slight sag between two pole tops.
/// Only the chunk that owns the starting pole stores the geometry.
fn emit_wire(a: Vec3, b: Vec3, ox: i32, oz: i32, size: i32, out: &mut Urban) {
    if !inside(a.x.floor() as i32, a.z.floor() as i32, ox, oz, size) {
        return;
    }
    let sag = vec3(0.0, -1.4, 0.0);
    let mid = (a + b) * 0.5 + sag;
    out.wires.push((a, mid));
    out.wires.push((mid, b));
}

use macroquad::prelude::*;

/// Enum representing different types of voxels in the world.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VoxelType {
    Air,
    Dirt,
    Grass,
    Sand,
    Stone,
    /// Exposed red brick ("tijolo à vista" / alvenaria).
    Brick,
    /// Bare cement: slabs, stairs, street paving.
    Concrete,
    /// Painted wall. The exact colour comes from the per-block `tone`.
    Wall,
    /// Corrugated zinc sheet (roofs).
    Zinc,
    /// Terracotta roof tiles ("telhas").
    Tile,
    /// Dark window glass.
    Glass,
    Wood,
    /// Blue plastic water tank ("caixa d'água").
    BluePlastic,
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
///
/// `tone` selects a variation from the material's palette (paint colours,
/// brick shades, zinc weathering...). It is a single byte: a block is
/// `kind + tone`, never a new enum variant per colour.
#[derive(Clone, Copy, Debug)]
pub struct VoxelStamp {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub kind: VoxelType,
    pub tone: u8,
}

impl VoxelStamp {
    /// Convenience constructor for the (very common) default tone.
    #[inline]
    pub fn new(x: i32, y: i32, z: i32, kind: VoxelType) -> Self {
        Self { x, y, z, kind, tone: 0 }
    }
}

impl VoxelBlock {
    /// Colour of a `(material, tone)` pair. Most materials have a handful of
    /// shades; walls use a vivid paint palette. `tone` wraps, so callers can
    /// hand it any hash without bounds-checking.
    pub fn color_for(kind: VoxelType, tone: u8) -> Color {
        let t = tone as usize;
        match kind {
            VoxelType::Air => Color::from_rgba(0, 0, 0, 0),
            VoxelType::Dirt => Color::from_rgba(102, 51, 0, 255),
            VoxelType::Grass => Color::from_rgba(88, 158, 62, 255),
            VoxelType::Sand => Color::from_rgba(227, 205, 152, 255),
            VoxelType::Stone => Color::from_rgba(126, 127, 132, 255),
            VoxelType::Leaves => Color::from_rgba(46, 139, 58, 255),
            VoxelType::Wood => Color::from_rgba(150, 75, 0, 255),
            VoxelType::Glass => match t % 2 {
                0 => Color::from_rgba(52, 74, 96, 255), // dark, unlit window
                _ => Color::from_rgba(74, 104, 130, 255), // sky reflection
            },
            // Red exposed brick, three kiln shades.
            VoxelType::Brick => match t % 3 {
                0 => Color::from_rgba(158, 74, 52, 255),
                1 => Color::from_rgba(138, 59, 42, 255),
                _ => Color::from_rgba(176, 90, 60, 255),
            },
            // Bare cement, slightly weathered.
            VoxelType::Concrete => match t % 3 {
                0 => Color::from_rgba(185, 183, 177, 255),
                1 => Color::from_rgba(161, 159, 153, 255),
                _ => Color::from_rgba(142, 140, 135, 255),
            },
            // Corrugated zinc: greys that read as weathered metal.
            VoxelType::Zinc => match t % 2 {
                0 => Color::from_rgba(138, 143, 148, 255),
                _ => Color::from_rgba(110, 116, 122, 255),
            },
            // Terracotta tiles.
            VoxelType::Tile => match t % 2 {
                0 => Color::from_rgba(181, 101, 74, 255),
                _ => Color::from_rgba(160, 82, 45, 255),
            },
            // Painted walls: the vivid favela palette.
            VoxelType::Wall => match t % 10 {
                0 => Color::from_rgba(232, 194, 74, 255),  // amarelo
                1 => Color::from_rgba(46, 111, 183, 255),  // azul
                2 => Color::from_rgba(78, 155, 88, 255),   // verde
                3 => Color::from_rgba(217, 139, 160, 255), // rosa
                4 => Color::from_rgba(224, 138, 60, 255),  // laranja
                5 => Color::from_rgba(237, 230, 218, 255), // branco/off-white
                6 => Color::from_rgba(63, 168, 160, 255),  // turquesa
                7 => Color::from_rgba(192, 57, 43, 255),   // vermelho
                8 => Color::from_rgba(126, 90, 168, 255),  // roxo
                _ => Color::from_rgba(120, 144, 156, 255), // cinza-azulado
            },
            VoxelType::BluePlastic => match t % 2 {
                0 => Color::from_rgba(46, 111, 196, 255),
                _ => Color::from_rgba(59, 130, 214, 255),
            },
        }
    }

    /// Number of distinct tones a material can use. Used by the generator to
    /// pick a stable variant from a hash without hard-coding the count.
    pub fn palette_len(kind: VoxelType) -> u8 {
        match kind {
            VoxelType::Wall => 10,
            VoxelType::Brick | VoxelType::Concrete => 3,
            VoxelType::Zinc | VoxelType::Tile | VoxelType::Glass | VoxelType::BluePlastic => 2,
            _ => 1,
        }
    }
}

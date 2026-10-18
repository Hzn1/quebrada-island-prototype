//! Biome classification: turns (altitude, humidity, local slope) into a
//! biome, and a biome into the block palette used by the chunk mesher.
//!
//! Everything is a pure function of world coordinates, so any chunk can
//! classify its columns without knowing anything about its neighbours.

use crate::config;
use crate::procedural::voxel_block::VoxelType;

/// Altitude above which terrain turns into the high, rocky biome
/// ("pedra nas montanhas mais altas").
pub const HIGHLAND_LEVEL: f32 = 24.0;

/// Slope (max height difference to a direct neighbour, in blocks) above
/// which a column is a cliff face and shows bare rock regardless of height.
const CLIFF_SLOPE: f32 = 3.0;

/// Humidity above which a mid-altitude column becomes dense forest.
const FOREST_HUMIDITY: f32 = 0.55;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    /// Underwater floor (sand/gravel shelf).
    Seabed,
    /// Sand strip along the coastline.
    Beach,
    /// Dry, gentle lowlands - grass and scattered rocks.
    Grassland,
    /// Humid slopes covered in denser vegetation.
    Forest,
    /// High altitude rocky ground with patchy grass.
    Highlands,
    /// Steep faces where soil cannot hold - exposed stone.
    Cliffs,
}

impl Biome {
    /// Classifies the biome of one surface column.
    ///
    /// * `height`   - quantized surface height (top of the ground, world units)
    /// * `humidity` - 0..1 moisture from the humidity noise map
    /// * `slope`    - max |Δheight| to the 4 direct neighbours, in blocks
    pub fn classify(height: f32, humidity: f32, slope: f32) -> Biome {
        if height < config::SEA_LEVEL - 0.5 {
            return Biome::Seabed;
        }
        if height < config::SEA_LEVEL + config::BEACH_WIDTH {
            return Biome::Beach;
        }
        // Steep ground sheds its soil -> rock, whatever the altitude
        if slope >= CLIFF_SLOPE {
            return Biome::Cliffs;
        }
        if height >= HIGHLAND_LEVEL {
            return Biome::Highlands;
        }
        if humidity >= FOREST_HUMIDITY {
            Biome::Forest
        } else {
            Biome::Grassland
        }
    }

    /// Block placed on the visible surface of this biome.
    /// `variant` is a 0..1 hash of the column, used by patchy biomes
    /// (e.g. highlands mix grass patches into the rock).
    pub fn surface_block(self, variant: f32) -> VoxelType {
        match self {
            Biome::Seabed | Biome::Beach => VoxelType::Sand,
            Biome::Cliffs => VoxelType::Stone,
            Biome::Grassland | Biome::Forest => VoxelType::Grass,
            // High mountains: mostly bare rock, some stubborn grass
            Biome::Highlands => {
                if variant < 0.3 {
                    VoxelType::Grass
                } else {
                    VoxelType::Stone
                }
            }
        }
    }

    /// Block used below the surface layer (side faces of cliffs/ledges).
    pub fn filler_block(self) -> VoxelType {
        match self {
            Biome::Seabed | Biome::Beach => VoxelType::Sand,
            Biome::Cliffs => VoxelType::Stone,
            _ => VoxelType::Dirt,
        }
    }

    /// How many blocks deep the surface material goes before transitioning
    /// to the filler block. `i32::MAX` means "all the way down" (cliffs are
    /// solid rock).
    pub fn top_depth(self) -> i32 {
        match self {
            Biome::Seabed | Biome::Beach => 3,
            Biome::Cliffs => i32::MAX,
            _ => 1,
        }
    }
}

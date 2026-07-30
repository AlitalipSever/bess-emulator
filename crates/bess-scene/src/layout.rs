//! GW-01 site geometry in metres, derived from the plant configuration.
//! Pure data: world positions and picking boxes; no GL, no state.
//!
//! Plan view (x to the right, z down, ground at y = 0):
//!
//! ```text
//!   row 1:  [C][C]P  [C][C]P  ...  (10 blocks)          fence
//!           ------------- road -------------  transformer  gantry -> grid
//!   row 0:  [C][C]P  [C][C]P  ...  (10 blocks)
//! ```

use bess_core::config::PlantConfig;

use crate::math::{Aabb, Ray, Vec3};

/// Container footprint: length (x), width (z), height.
pub const CONTAINER_SIZE: Vec3 = [6.1, 2.9, 2.5];
/// PCS skid: length (x), height, width (z).
pub const PCS_SIZE: Vec3 = [3.0, 2.3, 2.2];
/// Spacing between container centerlines inside a block, m.
pub const CONTAINER_PITCH_Z: f32 = 3.4;
/// Block pitch along the row, m.
pub const BLOCK_PITCH_X: f32 = 12.0;
/// Row centerline offset from the road axis, m.
pub const ROW_Z: f32 = 9.0;
/// Transformer position on the road axis, m.
pub const TRANSFORMER_X: f32 = 68.0;
/// Grid take-off gantry position, m.
pub const GANTRY_X: f32 = 76.0;
/// Gantry crossarm height, m.
pub const GANTRY_H: f32 = 8.0;

/// What a click can land on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// One battery container.
    Container {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
    },
    /// One PCS skid.
    Pcs {
        /// Block index.
        block: usize,
    },
    /// The main transformer.
    Transformer,
}

/// One power block placed in the world.
#[derive(Debug, Clone)]
pub struct BlockLayout {
    /// Row index (0 = south of the road, 1 = north).
    pub row: usize,
    /// Block center on the ground plane (y = 0).
    pub center: Vec3,
    /// World-space center of each container.
    pub container_centers: Vec<Vec3>,
    /// Picking box of each container.
    pub container_boxes: Vec<Aabb>,
    /// World-space center of the PCS skid.
    pub pcs_center: Vec3,
    /// Picking box of the PCS skid.
    pub pcs_box: Aabb,
    /// Outward direction of the container door faces (z sign).
    pub door_sign: f32,
}

/// The whole site, placed.
#[derive(Debug, Clone)]
pub struct SiteLayout {
    /// Power blocks in state-tree order.
    pub blocks: Vec<BlockLayout>,
    /// Main transformer center.
    pub transformer_center: Vec3,
    /// Main transformer picking box.
    pub transformer_box: Aabb,
    /// Light mast base positions.
    pub masts: Vec<Vec3>,
    /// Gravel apron: (center, size).
    pub gravel: (Vec3, Vec3),
    /// Road strip: (center, size).
    pub road: (Vec3, Vec3),
    /// Fence rectangle: (min x, min z, max x, max z).
    pub fence: (f32, f32, f32, f32),
}

impl SiteLayout {
    /// Place the site for a plant configuration. Blocks fill row 0 first,
    /// then row 1, matching state-tree index order.
    pub fn new(cfg: &PlantConfig) -> Self {
        let cols = cfg.blocks.div_ceil(2);
        let n_cont = cfg.containers_per_block;
        let mut blocks = Vec::with_capacity(cfg.blocks);
        for b in 0..cfg.blocks {
            let row = b / cols;
            let col = b % cols;
            let x = (col as f32 - (cols as f32 - 1.0) / 2.0) * BLOCK_PITCH_X;
            let z_row = if row == 0 { -ROW_Z } else { ROW_Z };
            // Doors face the road so an operator walking the road sees them.
            let door_sign = if row == 0 { 1.0 } else { -1.0 };

            let mut container_centers = Vec::with_capacity(n_cont);
            let mut container_boxes = Vec::with_capacity(n_cont);
            for c in 0..n_cont {
                let dz = (c as f32 - (n_cont as f32 - 1.0) / 2.0) * CONTAINER_PITCH_Z;
                let center = [x, CONTAINER_SIZE[1] / 2.0, z_row + dz];
                container_centers.push(center);
                container_boxes.push(Aabb::from_center_size(center, CONTAINER_SIZE));
            }

            let pcs_center = [
                x + CONTAINER_SIZE[0] / 2.0 + PCS_SIZE[0] / 2.0 + 1.2,
                PCS_SIZE[1] / 2.0,
                z_row,
            ];
            blocks.push(BlockLayout {
                row,
                center: [x, 0.0, z_row],
                container_centers,
                container_boxes,
                pcs_center,
                pcs_box: Aabb::from_center_size(pcs_center, PCS_SIZE),
                door_sign,
            });
        }

        let transformer_center = [TRANSFORMER_X, 1.6, 0.0];
        let half_x = (cols as f32) * BLOCK_PITCH_X / 2.0 + 8.0;
        let fence = (-half_x, -16.0, GANTRY_X + 8.0, 16.0);
        let gravel_cx = (fence.0 + fence.2) / 2.0;
        let gravel_sx = fence.2 - fence.0 + 6.0;

        SiteLayout {
            blocks,
            transformer_center,
            transformer_box: Aabb::from_center_size(transformer_center, [3.6, 3.2, 2.8]),
            masts: vec![
                [fence.0 + 3.0, 0.0, -13.0],
                [fence.0 + 3.0, 0.0, 13.0],
                [fence.2 - 20.0, 0.0, -13.0],
                [fence.2 - 20.0, 0.0, 13.0],
            ],
            gravel: ([gravel_cx, 0.0, 0.0], [gravel_sx, 1.0, 38.0]),
            road: ([gravel_cx, 0.0, 0.0], [gravel_sx - 10.0, 1.0, 5.0]),
            fence,
        }
    }

    /// Nearest object hit by a ray, if any.
    pub fn pick(&self, ray: &Ray) -> Option<Selection> {
        let mut best: Option<(f32, Selection)> = None;
        let mut consider = |t: Option<f32>, sel: Selection| {
            if let Some(t) = t {
                if best.is_none_or(|(bt, _)| t < bt) {
                    best = Some((t, sel));
                }
            }
        };
        for (b, block) in self.blocks.iter().enumerate() {
            for (c, cont) in block.container_boxes.iter().enumerate() {
                consider(
                    cont.hit(ray),
                    Selection::Container {
                        block: b,
                        container: c,
                    },
                );
            }
            consider(block.pcs_box.hit(ray), Selection::Pcs { block: b });
        }
        consider(self.transformer_box.hit(ray), Selection::Transformer);
        best.map(|(_, sel)| sel)
    }
}

#[cfg(test)]
mod tests {
    use super::{Selection, SiteLayout};
    use bess_core::config::PlantConfig;

    use crate::math::Ray;

    fn layout() -> SiteLayout {
        SiteLayout::new(&PlantConfig::gw01())
    }

    #[test]
    fn gw01_places_all_blocks_and_containers() {
        let l = layout();
        assert_eq!(l.blocks.len(), 20);
        assert!(l.blocks.iter().all(|b| b.container_boxes.len() == 2));
        // Two rows of ten.
        assert_eq!(l.blocks.iter().filter(|b| b.row == 0).count(), 10);
        assert_eq!(l.blocks.iter().filter(|b| b.row == 1).count(), 10);
    }

    #[test]
    fn container_boxes_do_not_overlap() {
        let l = layout();
        let boxes: Vec<_> = l
            .blocks
            .iter()
            .flat_map(|b| b.container_boxes.iter().copied())
            .collect();
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                let overlap = (0..3).all(|k| a.min[k] < b.max[k] && b.min[k] < a.max[k]);
                assert!(!overlap, "containers overlap");
            }
        }
    }

    #[test]
    fn ray_from_above_picks_the_right_container() {
        let l = layout();
        let target = l.blocks[7].container_centers[1];
        let ray = Ray {
            origin: [target[0], 50.0, target[2]],
            dir: [0.0, -1.0, 0.0],
        };
        assert_eq!(
            l.pick(&ray),
            Some(Selection::Container {
                block: 7,
                container: 1
            })
        );
    }

    #[test]
    fn ray_over_empty_ground_picks_nothing() {
        let l = layout();
        // The road axis between the rows is object-free.
        let ray = Ray {
            origin: [0.0, 50.0, 0.0],
            dir: [0.0, -1.0, 0.0],
        };
        assert_eq!(l.pick(&ray), None);
    }

    #[test]
    fn transformer_is_pickable() {
        let l = layout();
        let ray = Ray {
            origin: [super::TRANSFORMER_X, 50.0, 0.0],
            dir: [0.0, -1.0, 0.0],
        };
        assert_eq!(l.pick(&ray), Some(Selection::Transformer));
    }
}

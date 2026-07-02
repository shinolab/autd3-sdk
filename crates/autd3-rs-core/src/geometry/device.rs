use nalgebra::{Point3, UnitQuaternion, UnitVector3, Vector3};

#[derive(Clone, Debug)]
pub struct Device {
    idx: usize,
    rotation: UnitQuaternion<f32>,
    positions: Vec<Point3<f32>>,
    directions: Vec<UnitVector3<f32>>,
}

impl Device {
    pub(super) fn new(
        rotation: UnitQuaternion<f32>,
        positions: Vec<Point3<f32>>,
        directions: Vec<UnitVector3<f32>>,
    ) -> Self {
        debug_assert_eq!(positions.len(), directions.len());
        Self {
            idx: 0,
            rotation,
            positions,
            directions,
        }
    }

    pub(super) fn set_idx(&mut self, idx: usize) {
        self.idx = idx;
    }

    #[must_use]
    pub const fn idx(&self) -> usize {
        self.idx
    }

    #[must_use]
    pub const fn rotation(&self) -> UnitQuaternion<f32> {
        self.rotation
    }

    #[must_use]
    pub fn x_direction(&self) -> UnitVector3<f32> {
        self.rotation * Vector3::x_axis()
    }

    #[must_use]
    pub fn y_direction(&self) -> UnitVector3<f32> {
        self.rotation * Vector3::y_axis()
    }

    #[must_use]
    pub fn axial_direction(&self) -> UnitVector3<f32> {
        self.rotation * Vector3::z_axis()
    }

    #[must_use]
    pub const fn num_transducers(&self) -> usize {
        self.positions.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    #[must_use]
    pub fn positions(&self) -> &[Point3<f32>] {
        &self.positions
    }

    #[must_use]
    pub fn directions(&self) -> &[UnitVector3<f32>] {
        &self.directions
    }

    #[must_use]
    pub fn position(&self, index: usize) -> Point3<f32> {
        self.positions[index]
    }

    #[must_use]
    pub fn direction(&self, index: usize) -> UnitVector3<f32> {
        self.directions[index]
    }

    #[must_use]
    pub fn center(&self) -> Point3<f32> {
        let n = self.positions.len() as f32;
        let sum = self
            .positions
            .iter()
            .fold(Vector3::zeros(), |acc, p| acc + p.coords);
        Point3::from(sum / n)
    }
}

use nalgebra::{Point3, UnitVector3, Vector3};

#[derive(Clone, Debug)]
pub struct Device {
    positions: Vec<Point3<f32>>,
    directions: Vec<UnitVector3<f32>>,
}

impl Device {
    pub(super) fn new(positions: Vec<Point3<f32>>, directions: Vec<UnitVector3<f32>>) -> Self {
        debug_assert_eq!(positions.len(), directions.len());
        Self {
            positions,
            directions,
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
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

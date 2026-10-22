use crate::commands::Command;
use crate::commands::operation::Operation;
use crate::error::Error;
use crate::geometry::{Device, Geometry};

use super::frame::Frames;

pub struct Expansion<'g> {
    geometry: &'g Geometry,
    frames: &'g mut Frames,
}

impl<'g> Expansion<'g> {
    pub(crate) fn new(geometry: &'g Geometry, frames: &'g mut Frames) -> Self {
        Self { geometry, frames }
    }

    #[must_use]
    pub fn geometry(&self) -> &'g Geometry {
        self.geometry
    }

    pub fn push<C: Command>(&mut self, cmd: C) -> Result<&mut Self, Error> {
        tracing::trace!(command = std::any::type_name::<C>(), "expanding command");
        cmd.expand(self)?;
        Ok(self)
    }

    pub(crate) fn push_op<O: Operation>(&mut self, op: &O) -> Result<&mut Self, Error> {
        self.frames.push_encoded(op, self.geometry)?;
        Ok(self)
    }

    pub(crate) fn push_per_device<O, F>(&mut self, assign: F) -> Result<&mut Self, Error>
    where
        O: Operation,
        F: FnMut(&Device) -> Option<O>,
    {
        let assigned = self.frames.push_per_device(self.geometry, assign)?;
        tracing::trace!(
            operation = std::any::type_name::<O>(),
            assigned,
            num_devices = self.geometry.num_devices(),
            "encoded per-device operations"
        );
        Ok(self)
    }
}

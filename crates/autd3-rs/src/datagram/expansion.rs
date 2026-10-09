use crate::commands::Command;
use crate::commands::operation::Operation;
use crate::error::Error;
use crate::geometry::{Device, Geometry};

use super::each::{EachFrame, EachOps};

pub struct Expansion<'g, 'a> {
    geometry: &'g Geometry,
    pub(crate) ops: Vec<Box<dyn Operation + 'a>>,
}

impl<'g, 'a> Expansion<'g, 'a> {
    pub(crate) fn new(geometry: &'g Geometry) -> Self {
        Self {
            geometry,
            ops: Vec::new(),
        }
    }

    #[must_use]
    pub fn geometry(&self) -> &'g Geometry {
        self.geometry
    }

    pub fn push<C: Command<'a>>(&mut self, cmd: C) -> Result<&mut Self, Error> {
        tracing::trace!(command = std::any::type_name::<C>(), "expanding command");
        cmd.expand(self)?;
        Ok(self)
    }

    pub(crate) fn push_op<O: Operation + 'a>(&mut self, op: O) -> &mut Self {
        self.ops.push(Box::new(op));
        self
    }

    pub(crate) fn push_per_device<C, F>(&mut self, mut assign: F) -> Result<&mut Self, Error>
    where
        C: Command<'a>,
        F: FnMut(&Device) -> Option<C>,
    {
        let geometry = self.geometry;
        let devices: EachOps<'a> = geometry
            .iter()
            .map(|device| {
                assign(device).map_or_else(
                    || Ok(Vec::new()),
                    |cmd| {
                        let mut sub = Expansion::new(geometry);
                        cmd.expand(&mut sub)?;
                        Ok(sub.ops)
                    },
                )
            })
            .collect::<Result<_, Error>>()?;

        tracing::trace!(
            command = std::any::type_name::<C>(),
            assigned = devices.iter().filter(|ops| !ops.is_empty()).count(),
            num_devices = geometry.num_devices(),
            "expanding per-device commands"
        );

        self.ops.extend(
            EachFrame::flatten(devices).map(|frame| Box::new(frame) as Box<dyn Operation + 'a>),
        );
        Ok(self)
    }
}

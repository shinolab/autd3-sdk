use crate::commands::Command;
use crate::commands::operation::Operation;
use crate::error::Error;
use crate::geometry::{Device, Geometry};

use super::frame::Frames;

pub struct Expansion<'g> {
    geometry: &'g Geometry,
    frames: &'g mut Frames,
    device: Option<usize>,
    next_frame: usize,
}

impl<'g> Expansion<'g> {
    pub(crate) fn new(geometry: &'g Geometry, frames: &'g mut Frames) -> Self {
        let next_frame = frames.len();
        Self {
            geometry,
            frames,
            device: None,
            next_frame,
        }
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

    #[doc(hidden)]
    pub fn push_op<O: Operation>(&mut self, op: &O) -> Result<(), Error> {
        let index = self.next_frame;
        self.next_frame += 1;
        self.frames.encode_at(index, op, self.geometry, self.device)
    }

    #[doc(hidden)]
    pub fn push_each<C, F>(&mut self, mut assign: F) -> Result<(), Error>
    where
        C: Command,
        F: FnMut(&Device) -> Option<C>,
    {
        let start = self.next_frame;
        let mut end = start;
        let mut assigned = 0;
        for device in self.geometry {
            if self.device.is_some_and(|only| only != device.idx()) {
                continue;
            }
            let Some(cmd) = assign(device) else {
                continue;
            };
            assigned += 1;
            let mut per_device = Expansion {
                geometry: self.geometry,
                frames: self.frames,
                device: Some(device.idx()),
                next_frame: start,
            };
            let pushed = per_device.push(cmd).map(|_| ());
            end = end.max(per_device.next_frame);
            if pushed.is_err() {
                self.next_frame = end;
                return pushed;
            }
        }
        self.next_frame = end;
        tracing::trace!(
            command = std::any::type_name::<C>(),
            assigned,
            frames = end - start,
            num_devices = self.geometry.num_devices(),
            "expanded per-device commands"
        );
        Ok(())
    }
}

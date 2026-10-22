use std::marker::PhantomData;

use crate::commands::Command;
use crate::commands::operation::Operation;
use crate::error::Error;
use crate::geometry::{Device, Geometry};

use super::frame::Frames;

#[derive(Clone, Copy)]
enum Column {
    AllDevices,
    Device(usize),
    Discarded,
}

pub struct Expansion<'g, 'a> {
    geometry: &'g Geometry,
    frames: &'g mut Frames,
    column: Column,
    next_frame: usize,
    commands: PhantomData<&'a ()>,
}

impl<'g, 'a> Expansion<'g, 'a> {
    pub(crate) fn new(geometry: &'g Geometry, frames: &'g mut Frames) -> Self {
        let next_frame = frames.len();
        Self {
            geometry,
            frames,
            column: Column::AllDevices,
            next_frame,
            commands: PhantomData,
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

    pub(crate) fn push_op<O: Operation>(&mut self, op: &O) -> Result<&mut Self, Error> {
        let encoded = match self.column {
            Column::AllDevices => self.frames.push_encoded(op, self.geometry),
            Column::Device(device) => {
                self.frames
                    .pad_per_device(self.next_frame + 1, self.geometry.num_devices());
                self.frames
                    .encode_for_device(self.next_frame, op, &self.geometry[device])
            }
            Column::Discarded => Ok(()),
        };
        self.next_frame += 1;
        encoded?;
        Ok(self)
    }

    pub(crate) fn push_per_device<C, F>(&mut self, mut assign: F) -> Result<&mut Self, Error>
    where
        C: Command<'a>,
        F: FnMut(&Device) -> Option<C>,
    {
        let geometry = self.geometry;
        let start = self.next_frame;
        let mut end = start;
        let mut assigned = 0;
        let mut expanded = Ok(());
        for device in geometry {
            let Some(cmd) = assign(device) else {
                continue;
            };
            assigned += 1;
            let column = match self.column {
                Column::AllDevices => Column::Device(device.idx()),
                Column::Device(owner) if owner == device.idx() => Column::Device(owner),
                Column::Device(_) | Column::Discarded => Column::Discarded,
            };
            let mut device_expansion = Expansion {
                geometry,
                frames: &mut *self.frames,
                column,
                next_frame: start,
                commands: PhantomData,
            };
            expanded = cmd.expand(&mut device_expansion);
            end = end.max(device_expansion.next_frame);
            if expanded.is_err() {
                break;
            }
        }

        tracing::trace!(
            command = std::any::type_name::<C>(),
            assigned,
            num_devices = geometry.num_devices(),
            "expanding per-device commands"
        );

        if !matches!(self.column, Column::Discarded) {
            self.frames.pad_per_device(end, geometry.num_devices());
        }
        self.next_frame = end;
        expanded?;
        Ok(self)
    }
}

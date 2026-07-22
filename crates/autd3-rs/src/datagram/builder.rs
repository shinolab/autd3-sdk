use std::sync::{Arc, PoisonError};

use crate::commands::Command;
use crate::commands::operation::Operation;
use crate::error::Error;
use crate::geometry::{Device, Geometry};

use super::each::{EachOwned, each_reflect, each_slot_frames};
use super::frame::Frames;
use super::mirror::{Mirror, MirrorHandle};

enum Step<'a> {
    Op(Box<dyn Operation + 'a>),
    Each {
        devices: Vec<Vec<Box<dyn Operation + 'a>>>,
    },
}

pub struct DatagramBuilder<'a> {
    geometry: Arc<Geometry>,
    ops: Vec<Step<'a>>,
    mirror: Option<MirrorHandle>,
}

impl<'a> DatagramBuilder<'a> {
    #[must_use]
    pub fn new(geometry: Arc<Geometry>) -> Self {
        Self {
            geometry,
            ops: Vec::new(),
            mirror: None,
        }
    }

    #[must_use]
    pub(crate) fn with_mirror(geometry: Arc<Geometry>, mirror: MirrorHandle) -> Self {
        Self {
            geometry,
            ops: Vec::new(),
            mirror: Some(mirror),
        }
    }

    pub fn push<C: Command<'a>>(&mut self, cmd: C) -> &mut Self {
        cmd.expand(self);
        self
    }

    pub fn push_each<C, F>(&mut self, mut assign: F) -> &mut Self
    where
        C: Command<'a>,
        F: FnMut(&Device) -> Option<C>,
    {
        let geometry = Arc::clone(&self.geometry);
        let num_devices = geometry.num_devices();
        let new_devices: Vec<Vec<Box<dyn Operation + 'a>>> = geometry
            .iter()
            .map(|device| {
                assign(device).map_or_else(Vec::new, |cmd| {
                    let mut sub = DatagramBuilder::new(Arc::clone(&geometry));
                    cmd.expand(&mut sub);
                    sub.take_ops()
                })
            })
            .collect();

        let fuse = matches!(
            self.ops.last(),
            Some(Step::Each { devices }) if (0..num_devices)
                .all(|d| devices[d].is_empty() || new_devices[d].is_empty())
        );
        if fuse {
            if let Some(Step::Each { devices }) = self.ops.last_mut() {
                for (device, ops) in new_devices.into_iter().enumerate() {
                    if !ops.is_empty() {
                        devices[device] = ops;
                    }
                }
            }
        } else {
            self.ops.push(Step::Each {
                devices: new_devices,
            });
        }
        self
    }

    pub(crate) fn push_op<O: Operation + 'a>(&mut self, op: O) -> &mut Self {
        self.ops.push(Step::Op(Box::new(op)));
        self
    }

    pub(crate) fn take_ops(self) -> Vec<Box<dyn Operation + 'a>> {
        self.ops
            .into_iter()
            .map(|step| match step {
                Step::Op(op) => op,
                Step::Each { devices } => {
                    let slot_frames = each_slot_frames(&devices);
                    Box::new(EachOwned {
                        devices,
                        slot_frames,
                    }) as Box<dyn Operation + 'a>
                }
            })
            .collect()
    }

    pub fn build(&self) -> Result<Frames, Error> {
        let mut out = Frames::default();
        self.build_into(&mut out)?;
        Ok(out)
    }

    pub fn build_into(&self, out: &mut Frames) -> Result<(), Error> {
        out.clear();

        let mut guard = self
            .mirror
            .as_ref()
            .filter(|handle| handle.enabled)
            .map(|handle| handle.state.lock().unwrap_or_else(PoisonError::into_inner));

        let mut work = match guard.as_deref() {
            Some(Mirror::Synced(states)) => Some(states.clone()),
            _ => None,
        };

        for step in &self.ops {
            match step {
                Step::Op(op) => {
                    out.push_op(op.as_ref(), &self.geometry)?;
                    if let Some(work) = work.as_mut() {
                        for (device, state) in work.iter_mut().enumerate() {
                            op.reflect(device, state)?;
                        }
                    }
                }
                Step::Each { devices } => {
                    out.push_each_step(devices, &self.geometry)?;
                    if let Some(work) = work.as_mut() {
                        for (device, state) in work.iter_mut().enumerate() {
                            each_reflect(devices, device, state)?;
                        }
                    }
                }
            }
        }

        if let (Some(guard), Some(work)) = (guard.as_mut(), work) {
            **guard = Mirror::Synced(work);
        }
        Ok(())
    }
}

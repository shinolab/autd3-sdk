use std::sync::{Arc, PoisonError};

use crate::commands::Command;
use crate::commands::operation::Operation;
use crate::error::{Error, PayloadError};
use crate::geometry::{Device, Geometry};

use super::each::{EachFrame, EachOps, each_reflect};
use super::frame::Frames;
use super::mirror::{Mirror, MirrorHandle};

enum Step<'a> {
    Op(Box<dyn Operation + 'a>),
    Each { devices: EachOps<'a> },
}

pub struct DatagramBuilder<'a> {
    geometry: Arc<Geometry>,
    ops: Vec<Step<'a>>,
    invalid: Option<PayloadError>,
    mirror: Option<MirrorHandle>,
}

impl<'a> DatagramBuilder<'a> {
    #[must_use]
    pub fn new(geometry: Arc<Geometry>) -> Self {
        Self {
            geometry,
            ops: Vec::new(),
            invalid: None,
            mirror: None,
        }
    }

    #[must_use]
    pub(crate) fn with_mirror(geometry: Arc<Geometry>, mirror: MirrorHandle) -> Self {
        Self {
            geometry,
            ops: Vec::new(),
            invalid: None,
            mirror: Some(mirror),
        }
    }

    pub fn push<C: Command<'a>>(&mut self, cmd: C) -> &mut Self {
        tracing::trace!(command = std::any::type_name::<C>(), "pushed command");
        cmd.expand(self);
        self
    }

    pub(crate) fn reject(&mut self, e: PayloadError) -> &mut Self {
        tracing::debug!(error = %e, "command rejected; build will fail");
        self.invalid.get_or_insert(e);
        self
    }

    pub fn push_each<C, F>(&mut self, mut assign: F) -> &mut Self
    where
        C: Command<'a>,
        F: FnMut(&Device) -> Option<C>,
    {
        let geometry = Arc::clone(&self.geometry);
        let num_devices = geometry.num_devices();
        let mut invalid = None;
        let new_devices: EachOps<'a> = geometry
            .iter()
            .map(|device| {
                assign(device).map_or_else(Vec::new, |cmd| {
                    let mut sub = DatagramBuilder::new(Arc::clone(&geometry));
                    cmd.expand(&mut sub);
                    invalid = invalid.or(sub.invalid);
                    sub.take_ops()
                })
            })
            .collect();
        if let Some(e) = invalid {
            self.reject(e);
        }

        tracing::trace!(
            command = std::any::type_name::<C>(),
            assigned = new_devices.iter().filter(|ops| !ops.is_empty()).count(),
            num_devices,
            "pushed per-device commands"
        );

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
            .flat_map(|step| match step {
                Step::Op(op) => vec![op],
                Step::Each { devices } => EachFrame::flatten(devices)
                    .map(|frame| Box::new(frame) as Box<dyn Operation + 'a>)
                    .collect(),
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

        if let Some(e) = self.invalid {
            return Err(e.into());
        }

        let mut guard = self
            .mirror
            .as_ref()
            .filter(|handle| handle.enabled)
            .map(|handle| handle.state.lock().unwrap_or_else(PoisonError::into_inner));

        let mut work = match guard.as_deref() {
            Some(Mirror::Synced(states)) => Some(states.clone()),
            _ => None,
        };
        if guard.is_some() && work.is_none() {
            tracing::debug!("mirror is desynced; skipping state validation");
        }

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

        let reflected = work.is_some();
        if let (Some(guard), Some(work)) = (guard.as_mut(), work) {
            **guard = Mirror::Synced(work);
        }
        tracing::trace!(
            steps = self.ops.len(),
            frames = out.len(),
            mirror_reflected = reflected,
            "built frames"
        );
        Ok(())
    }
}

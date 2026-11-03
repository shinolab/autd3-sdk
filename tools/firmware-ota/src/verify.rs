use anyhow::Result;

use autd3_cpu_wire::update::RunningImage;
use autd3_rs::Version;

use crate::error::{NotOnTrial, OffTrial, TrialNotRunning};

#[derive(Clone, Copy)]
pub(crate) enum Expect<'a> {
    Exactly(Version),
    ChangedFrom(&'a [Version]),
}

pub(crate) fn describe_running_image(image: Option<RunningImage>) -> &'static str {
    match image {
        Some(RunningImage::Unconfirmed) => "unconfirmed trial image",
        Some(RunningImage::Confirmed) => "confirmed image",
        Some(_) => "image of unknown state",
        None => "image state not reported by this firmware",
    }
}

pub(crate) fn check_expectation(versions: &[Version], expect: Expect<'_>) -> Result<()> {
    match expect {
        Expect::Exactly(expected) => {
            if let Some((device, &found)) =
                versions.iter().enumerate().find(|&(_, v)| *v != expected)
            {
                return Err(TrialNotRunning {
                    device,
                    found,
                    expected: format!("the release is {expected}"),
                }
                .into());
            }
        }
        Expect::ChangedFrom(_) => {}
    }
    Ok(())
}

pub(crate) fn check_trial(
    images: &[Option<RunningImage>],
    versions: &[Version],
    expect: Expect<'_>,
) -> Result<()> {
    fn off_trial_by_version(versions: &[Version], expect: Expect<'_>) -> Vec<usize> {
        let Expect::ChangedFrom(before) = expect else {
            return Vec::new();
        };
        if versions.windows(2).all(|pair| pair[0] == pair[1]) {
            if before == versions {
                eprintln!(
                    "warning: the CPU firmware version did not change across the reboot; \
                     this is expected only if the same version was written again, otherwise \
                     the trial image did not boot and the previous image is running"
                );
            }
            return Vec::new();
        }
        let unchanged: Vec<usize> = (0..versions.len())
            .filter(|&device| before.get(device) == Some(&versions[device]))
            .collect();
        if unchanged.is_empty() {
            (0..versions.len()).collect()
        } else {
            unchanged
        }
    }

    let off_trial = if images.iter().any(Option::is_some) {
        (0..images.len())
            .filter(|&device| images[device] != Some(RunningImage::Unconfirmed))
            .collect()
    } else {
        eprintln!(
            "warning: this CPU firmware cannot report whether the trial image is running; \
             only the versions are compared"
        );
        off_trial_by_version(versions, expect)
    };
    if off_trial.is_empty() {
        return Ok(());
    }
    Err(NotOnTrial {
        devices: off_trial
            .into_iter()
            .map(|device| OffTrial {
                device,
                version: versions[device],
                image: images[device],
            })
            .collect(),
    }
    .into())
}

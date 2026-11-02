use anyhow::Result;

use autd3_cpu_wire::update::RunningImage;

pub(crate) type Version = [u8; 3];

#[derive(Clone, Copy)]
pub(crate) enum Expect<'a> {
    Exactly(Version),
    ChangedFrom(&'a [Version]),
    Anything,
}

#[derive(Debug, thiserror::Error)]
#[error(
    "device {device} runs CPU firmware {}.{}.{} after the reboot, but {expected}; the trial image is not the one running, so it is left unconfirmed and the previous image stays",
    found[0], found[1], found[2]
)]
pub(crate) struct TrialNotRunning {
    device: usize,
    found: Version,
    expected: String,
}

#[derive(Debug)]
struct OffTrial {
    device: usize,
    version: Version,
    image: Option<RunningImage>,
}

impl std::fmt::Display for OffTrial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "device {} ({}, CPU firmware {})",
            self.device,
            describe_running_image(self.image),
            fmt_version(self.version)
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "not every device runs the trial image after the reboot: {} fell back to the previous image. \
     No device was confirmed, so the devices that do run the trial image fall back as well at \
     their next reset; run the update again, or reset every device (`--reboot-only` or a power \
     cycle) to bring them all back to the previous image",
    devices.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
)]
pub(crate) struct NotOnTrial {
    devices: Vec<OffTrial>,
}

pub(crate) fn fmt_version([major, minor, patch]: Version) -> String {
    format!("{major}.{minor}.{patch}")
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
                    expected: format!("the release is {}", fmt_version(expected)),
                }
                .into());
            }
        }
        Expect::ChangedFrom(_) | Expect::Anything => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exact_expectation_names_the_first_device_running_something_else() {
        assert!(check_expectation(&[[0, 9, 0], [0, 9, 0]], Expect::Exactly([0, 9, 0])).is_ok());
        let err =
            check_expectation(&[[0, 9, 0], [0, 6, 1]], Expect::Exactly([0, 9, 0])).unwrap_err();
        let err = err.downcast_ref::<TrialNotRunning>().unwrap();
        assert_eq!((err.device, err.found), (1, [0, 6, 1]));
    }

    #[test]
    fn an_unchanged_version_is_only_a_warning() {
        let before = [[0, 9, 0]];
        assert!(check_expectation(&[[0, 9, 0]], Expect::ChangedFrom(&before)).is_ok());
        assert!(check_expectation(&[[0, 9, 1]], Expect::ChangedFrom(&before)).is_ok());
        assert!(check_expectation(&[[0, 6, 1]], Expect::Anything).is_ok());
        assert!(check_trial(&[None], &[[0, 9, 0]], Expect::ChangedFrom(&before)).is_ok());
        assert!(check_trial(&[None], &[[0, 9, 1]], Expect::ChangedFrom(&before)).is_ok());
    }

    fn off_trial(
        images: &[Option<RunningImage>],
        versions: &[Version],
        expect: Expect<'_>,
    ) -> Vec<usize> {
        match check_trial(images, versions, expect) {
            Ok(()) => Vec::new(),
            Err(e) => e
                .downcast_ref::<NotOnTrial>()
                .unwrap()
                .devices
                .iter()
                .map(|d| d.device)
                .collect(),
        }
    }

    const TRIAL: Option<RunningImage> = Some(RunningImage::Unconfirmed);
    const CONFIRMED: Option<RunningImage> = Some(RunningImage::Confirmed);
    const SAME: [Version; 3] = [[0, 10, 0]; 3];
    const NONE: [usize; 0] = [];

    #[test]
    fn every_device_must_report_an_unconfirmed_trial() {
        let rewritten = Expect::ChangedFrom(&SAME);
        assert_eq!(off_trial(&[TRIAL; 3], &SAME, rewritten), NONE);
        assert_eq!(off_trial(&[TRIAL, CONFIRMED, TRIAL], &SAME, rewritten), [1]);
        assert_eq!(
            off_trial(
                &[Some(RunningImage::Unknown), TRIAL, CONFIRMED],
                &SAME,
                rewritten
            ),
            [0, 2]
        );
        assert_eq!(off_trial(&[CONFIRMED; 3], &SAME, rewritten), [0, 1, 2]);
        assert_eq!(
            off_trial(&[TRIAL, CONFIRMED], &SAME[..2], Expect::Exactly(SAME[0])),
            [1]
        );
    }

    #[test]
    fn a_device_whose_firmware_cannot_report_among_ones_that_can_is_off_trial() {
        let versions = [[0, 11, 0], [0, 10, 0], [0, 11, 0]];
        assert_eq!(
            off_trial(&[TRIAL, None, TRIAL], &versions, Expect::ChangedFrom(&SAME)),
            [1]
        );
    }

    #[test]
    fn firmware_that_cannot_report_falls_back_to_the_versions() {
        let before = [[0, 10, 1]; 3];
        let unreported = [None; 3];
        assert_eq!(
            off_trial(&unreported, &SAME, Expect::ChangedFrom(&before)),
            NONE
        );
        assert_eq!(
            off_trial(&unreported, &SAME, Expect::ChangedFrom(&SAME)),
            NONE
        );
        assert_eq!(
            off_trial(
                &unreported,
                &[[0, 10, 0], [0, 10, 1], [0, 10, 0]],
                Expect::ChangedFrom(&before)
            ),
            [1]
        );
        assert_eq!(
            off_trial(
                &unreported,
                &[[0, 10, 0], [0, 10, 2], [0, 10, 3]],
                Expect::ChangedFrom(&before)
            ),
            [0, 1, 2]
        );
        assert_eq!(
            off_trial(&unreported, &SAME, Expect::Exactly(SAME[0])),
            NONE
        );
    }

    #[test]
    fn the_error_names_each_device_that_is_off_trial() {
        let err = check_trial(&[TRIAL, CONFIRMED], &SAME[..2], Expect::ChangedFrom(&SAME))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("device 1 (confirmed image, CPU firmware 0.10.0)"),
            "{err}"
        );
        assert!(!err.contains("device 0"), "{err}");
    }
}

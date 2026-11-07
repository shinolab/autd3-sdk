use std::time::Instant;

#[cfg(any(windows, test))]
mod cmsg;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub(super) use linux::Stamping;
#[cfg(target_os = "macos")]
pub(super) use macos::Stamping;
#[cfg(windows)]
pub(super) use windows::Stamping;

fn arrival_instant(now: Instant, opened_at: Instant, queued_ns: Option<i64>) -> Instant {
    queued_ns
        .and_then(|queued_ns| u64::try_from(queued_ns).ok())
        .and_then(|queued_ns| now.checked_sub(std::time::Duration::from_nanos(queued_ns)))
        .filter(|&arrival| arrival >= opened_at)
        .unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_at_and_opened_a_second_before() -> (Instant, Instant) {
        let now = Instant::now();
        (
            now,
            now.checked_sub(std::time::Duration::from_secs(1)).unwrap(),
        )
    }

    #[test]
    fn a_reply_queued_in_the_kernel_is_stamped_at_its_arrival() {
        let (now, opened_at) = read_at_and_opened_a_second_before();
        assert_eq!(
            arrival_instant(now, opened_at, Some(5_000)),
            now.checked_sub(std::time::Duration::from_micros(5))
                .unwrap()
        );
    }

    #[test]
    fn a_reply_without_a_kernel_timestamp_is_stamped_when_read() {
        let (now, opened_at) = read_at_and_opened_a_second_before();
        assert_eq!(arrival_instant(now, opened_at, None), now);
    }

    #[test]
    fn a_kernel_timestamp_later_than_the_read_is_ignored() {
        let (now, opened_at) = read_at_and_opened_a_second_before();
        assert_eq!(arrival_instant(now, opened_at, Some(-1)), now);
    }

    #[test]
    fn a_kernel_timestamp_earlier_than_the_open_is_ignored() {
        let (now, opened_at) = read_at_and_opened_a_second_before();
        assert_eq!(arrival_instant(now, opened_at, Some(2_000_000_000)), now);
    }
}

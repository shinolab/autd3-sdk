pub(super) fn wsa_rx_timestamp(control: &[u8]) -> Option<u64> {
    let word = size_of::<usize>();
    let header = word + 2 * size_of::<i32>();
    let data_offset = header.next_multiple_of(word);
    let mut rest = control;
    while rest.len() >= header {
        let cmsg_len = usize::from_ne_bytes(rest[..word].try_into().ok()?);
        let level = i32::from_ne_bytes(rest[word..word + 4].try_into().ok()?);
        let kind = i32::from_ne_bytes(rest[word + 4..word + 8].try_into().ok()?);
        if level == 0xFFFF && kind == 0x300A {
            return Some(u64::from_ne_bytes(
                rest.get(data_offset..data_offset + 8)?.try_into().ok()?,
            ));
        }
        rest = rest.get(cmsg_len.max(header).next_multiple_of(word)..)?;
    }
    None
}

pub(super) fn queued_ns_from_qpc(qpc_now: i64, stamp: u64, qpc_per_second: i64) -> Option<i64> {
    let queued = i128::from(qpc_now) - i128::from(stamp);
    i64::try_from((queued * 1_000_000_000).checked_div(i128::from(qpc_per_second))?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wsa_cmsg(level: i32, kind: i32, data: &[u8]) -> Vec<u8> {
        let word = size_of::<usize>();
        let data_offset = (word + 8).next_multiple_of(word);
        let mut cmsg = Vec::new();
        cmsg.extend_from_slice(&(data_offset + data.len()).to_ne_bytes());
        cmsg.extend_from_slice(&level.to_ne_bytes());
        cmsg.extend_from_slice(&kind.to_ne_bytes());
        cmsg.resize(data_offset, 0);
        cmsg.extend_from_slice(data);
        cmsg.resize(cmsg.len().next_multiple_of(word), 0);
        cmsg
    }

    #[test]
    fn the_wsa_receive_timestamp_is_read_from_its_control_message() {
        let control = wsa_cmsg(0xFFFF, 0x300A, &1_234_u64.to_ne_bytes());
        assert_eq!(wsa_rx_timestamp(&control), Some(1_234));
    }

    #[test]
    fn other_wsa_control_messages_before_the_timestamp_are_skipped() {
        let mut control = wsa_cmsg(41, 50, &[7; 20]);
        control.extend(wsa_cmsg(0xFFFF, 0x300A, &5_678_u64.to_ne_bytes()));
        assert_eq!(wsa_rx_timestamp(&control), Some(5_678));
    }

    #[test]
    fn a_truncated_wsa_timestamp_is_absent() {
        let control = wsa_cmsg(0xFFFF, 0x300A, &1_234_u64.to_ne_bytes());
        assert_eq!(wsa_rx_timestamp(&control[..control.len() - 1]), None);
        assert_eq!(wsa_rx_timestamp(&[]), None);
    }

    #[test]
    fn qpc_ticks_queued_are_converted_to_nanoseconds() {
        assert_eq!(
            queued_ns_from_qpc(10_000_050, 10_000_000, 10_000_000),
            Some(5_000)
        );
        assert_eq!(
            queued_ns_from_qpc(10_000_000, 10_000_050, 10_000_000),
            Some(-5_000)
        );
        assert_eq!(queued_ns_from_qpc(10_000_050, 10_000_000, 0), None);
    }
}

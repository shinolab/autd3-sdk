use std::collections::VecDeque;

use autd3_rs::link::message::LINK_VERSION;

pub const REMEMBERED_SENDS: usize = 64;

#[derive(Debug, Default)]
pub struct MsgIds {
    sent: VecDeque<(u16, u16)>,
}

impl MsgIds {
    pub fn remember(&mut self, bus: u16, client: u16) {
        if self.sent.len() == REMEMBERED_SENDS {
            self.sent.pop_front();
        }
        self.sent.push_back((bus, client));
    }

    #[must_use]
    pub fn client_of(&self, bus: u16) -> Option<u16> {
        self.sent
            .iter()
            .rev()
            .find(|&&(sent, _)| sent == bus)
            .map(|&(_, client)| client)
    }
}

pub fn admission(
    version: u16,
    num_devices: u16,
    serving: Option<usize>,
    attached: bool,
) -> Result<(), String> {
    if version != LINK_VERSION {
        return Err(format!(
            "the client speaks link version {version}, but the server speaks {LINK_VERSION}"
        ));
    }
    let Some(serving) = serving else {
        return Err("the server is still enumerating the devices".to_owned());
    };
    if attached {
        return Err("another client is attached".to_owned());
    }
    if usize::from(num_devices) != serving {
        return Err(format!(
            "the geometry has {num_devices} devices, but the server serves {serving}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_maps_back_to_the_msg_id_the_client_chose() {
        let mut ids = MsgIds::default();
        ids.remember(100, 7);
        ids.remember(101, 8);
        assert_eq!(ids.client_of(100), Some(7));
        assert_eq!(ids.client_of(101), Some(8));
        assert_eq!(ids.client_of(102), None);
    }

    #[test]
    fn only_the_latest_sends_are_remembered() {
        let mut ids = MsgIds::default();
        for bus in 0..=u16::try_from(REMEMBERED_SENDS).unwrap() {
            ids.remember(bus, bus + 1000);
        }
        assert_eq!(ids.client_of(0), None);
        assert_eq!(ids.client_of(1), Some(1001));
        assert_eq!(ids.sent.len(), REMEMBERED_SENDS);
    }

    #[test]
    fn a_reused_bus_msg_id_maps_to_its_latest_send() {
        let mut ids = MsgIds::default();
        ids.remember(5, 1);
        ids.remember(5, 2);
        assert_eq!(ids.client_of(5), Some(2));
    }

    #[test]
    fn a_client_is_admitted_only_by_a_serving_free_server_of_the_same_size() {
        assert_eq!(admission(LINK_VERSION, 2, Some(2), false), Ok(()));
        assert!(
            admission(LINK_VERSION, 2, None, false)
                .unwrap_err()
                .contains("enumerating")
        );
        assert!(
            admission(LINK_VERSION, 2, Some(2), true)
                .unwrap_err()
                .contains("another client")
        );
        assert!(
            admission(LINK_VERSION, 3, Some(2), false)
                .unwrap_err()
                .contains("3 devices")
        );
        assert!(
            admission(LINK_VERSION + 1, 2, Some(2), false)
                .unwrap_err()
                .contains("link version")
        );
    }
}

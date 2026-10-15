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

#[derive(Debug)]
pub struct Attachment<B, S> {
    bus: Option<B>,
    session: Option<S>,
}

impl<B, S> Default for Attachment<B, S> {
    fn default() -> Self {
        Self {
            bus: None,
            session: None,
        }
    }
}

impl<B, S> Attachment<B, S> {
    #[must_use]
    pub fn session(&self) -> Option<&S> {
        self.session.as_ref()
    }

    #[must_use]
    pub fn bus_of(&self, attached: impl FnOnce(&S) -> bool) -> Option<&B> {
        self.session
            .as_ref()
            .is_some_and(attached)
            .then_some(self.bus.as_ref())
            .flatten()
    }

    pub fn serve(&mut self, bus: B) {
        self.bus = Some(bus);
    }

    pub fn retire(&mut self) -> Option<S> {
        self.bus = None;
        self.session.take()
    }

    pub fn detach(&mut self, attached: impl FnOnce(&S) -> bool) {
        if self.session.as_ref().is_some_and(attached) {
            self.session = None;
        }
    }
}

impl<B: Clone, S> Attachment<B, S> {
    pub fn attach<E>(
        &mut self,
        session: S,
        admit: impl FnOnce(Option<&B>, bool) -> Result<(), E>,
    ) -> Result<Option<B>, E> {
        admit(self.bus.as_ref(), self.session.is_some())?;
        let Some(bus) = self.bus.clone() else {
            return Ok(None);
        };
        self.session = Some(session);
        Ok(Some(bus))
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

    fn admit(serving: Option<&usize>, attached: bool) -> Result<(), String> {
        admission(LINK_VERSION, 2, serving.copied(), attached)
    }

    #[test]
    fn an_attach_after_the_bus_retires_is_refused() {
        let mut attachment = Attachment::<usize, u8>::default();
        attachment.serve(2);
        assert_eq!(attachment.retire(), None);
        assert_eq!(
            attachment.attach(1, admit),
            Err(admit(None, false).unwrap_err())
        );
        assert_eq!(attachment.bus, None);
        assert_eq!(attachment.session, None);
    }

    #[test]
    fn a_retiring_bus_takes_the_session_attached_before_it() {
        let mut attachment = Attachment::<usize, u8>::default();
        attachment.serve(2);
        assert_eq!(attachment.attach(1, admit), Ok(Some(2)));
        assert_eq!(attachment.retire(), Some(1));
        assert_eq!(attachment.bus, None);
        assert_eq!(attachment.session, None);
    }

    #[test]
    fn a_session_is_never_attached_without_a_bus() {
        let mut attachment = Attachment::<usize, u8>::default();
        assert_eq!(attachment.attach(1, |_, _| Ok::<_, String>(())), Ok(None));
        assert_eq!(attachment.session, None);
    }

    #[test]
    fn a_second_client_is_refused_while_one_is_attached() {
        let mut attachment = Attachment::<usize, u8>::default();
        attachment.serve(2);
        assert_eq!(attachment.attach(1, admit), Ok(Some(2)));
        assert_eq!(
            attachment.attach(2, admit),
            Err(admit(Some(&2), true).unwrap_err())
        );
        assert_eq!(attachment.session, Some(1));
    }

    #[test]
    fn only_the_attached_session_detaches() {
        let mut attachment = Attachment::<usize, u8>::default();
        attachment.serve(2);
        assert_eq!(attachment.attach(1, admit), Ok(Some(2)));
        attachment.detach(|&attached| attached == 2);
        assert_eq!(attachment.session, Some(1));
        attachment.detach(|&attached| attached == 1);
        assert_eq!(attachment.session, None);
        assert_eq!(attachment.bus, Some(2));
    }

    #[test]
    fn a_detached_session_does_not_reach_the_next_bus() {
        let mut attachment = Attachment::<usize, u8>::default();
        attachment.serve(2);
        assert_eq!(attachment.attach(1, admit), Ok(Some(2)));
        assert_eq!(attachment.bus_of(|&attached| attached == 1), Some(&2));
        assert_eq!(attachment.retire(), Some(1));
        attachment.serve(2);
        assert_eq!(attachment.bus_of(|&attached| attached == 1), None);
        assert_eq!(attachment.attach(3, admit), Ok(Some(2)));
        assert_eq!(attachment.bus_of(|&attached| attached == 1), None);
        assert_eq!(attachment.bus_of(|&attached| attached == 3), Some(&2));
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

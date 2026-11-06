use std::collections::VecDeque;

use crate::error::Error;
use crate::response::Response;

use super::Client;

#[derive(Debug, Default)]
pub struct InflightFrames {
    frames: VecDeque<(u16, Option<Response>)>,
}

impl InflightFrames {
    pub fn push(&mut self, msg_id: u16) {
        self.frames.push_back((msg_id, None));
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub async fn collect_oldest(&mut self, client: &Client) -> Result<Option<Response>, Error> {
        while let Some((_, oldest)) = self.frames.front()
            && !oldest.as_ref().is_some_and(Response::has_all_replies)
        {
            let reply = client.recv().await?;
            if let Some((_, response)) = self
                .frames
                .iter_mut()
                .find(|(msg_id, _)| *msg_id == reply.msg_id)
            {
                let response =
                    response.get_or_insert_with(|| Response::for_devices(client.num_devices()));
                if !response.replied_devices().contains(reply.device) {
                    response.record_reply(reply);
                }
            }
        }
        Ok(self.frames.pop_front().and_then(|(_, response)| response))
    }
}

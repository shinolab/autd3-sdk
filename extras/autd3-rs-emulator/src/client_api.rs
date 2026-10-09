use autd3_rs::commands::Command;
use autd3_rs::{Client, Frame};

#[allow(async_fn_in_trait)]
pub trait ClientApi {
    type Error;

    async fn send<'a, C: Command<'a>>(&mut self, cmd: C) -> Result<(), Self::Error>;

    async fn send_frame(&mut self, frame: Frame<'_>) -> Result<(), Self::Error>;
}

impl ClientApi for Client {
    type Error = autd3_rs::error::Error;

    async fn send<'a, C: Command<'a>>(&mut self, cmd: C) -> Result<(), Self::Error> {
        Client::send(self, cmd).await
    }

    async fn send_frame(&mut self, frame: Frame<'_>) -> Result<(), Self::Error> {
        Client::send_frame(self, frame).await?.await?.check()
    }
}

use autd3_rs::{Client, DatagramBuilder, Frame};

#[allow(async_fn_in_trait)]
pub trait ClientApi {
    type Error;

    fn datagram_builder<'a>(&self) -> DatagramBuilder<'a>;

    async fn send_checked(&mut self, frame: Frame<'_>) -> Result<(), Self::Error>;
}

impl ClientApi for Client {
    type Error = autd3_rs::error::Error;

    fn datagram_builder<'a>(&self) -> DatagramBuilder<'a> {
        Client::datagram_builder(self)
    }

    async fn send_checked(&mut self, frame: Frame<'_>) -> Result<(), Self::Error> {
        Client::send_checked(self, frame).await
    }
}

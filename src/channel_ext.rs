use std::time::Duration;

use smol::{
    channel::{Receiver, RecvError},
    future::FutureExt,
    Timer,
};

/// An extension trait that adds functionality to smol channel receivers.
pub trait ReceiverExt<T> {
    /// Get the newest element in the channel, discarding all previous messages. Blocks if there
    /// are no messages in the channel.
    async fn recv_last(&self) -> Result<T, RecvError>;

    /// Same as recv_last, but returns Ok(None) if the channel is empty.
    async fn recv_maybe_last(&self) -> Result<Option<T>, RecvError>;

    /// Receive an item from the channel or panic with "Did not receive initial value in time" if we
    /// don't get anything within the given timeout.
    async fn recv_or_panic_after_timeout(&self, timeout: Duration) -> Result<T, RecvError>;
}

impl<T> ReceiverExt<T> for Receiver<T> {
    async fn recv_last(&self) -> Result<T, RecvError> {
        let len = self.len();

        // remove first len - 1 messages
        if len > 1 {
            for _ in 0..(len - 1) {
                let _ = self.recv().await?;
            }
        }

        self.recv().await
    }

    async fn recv_maybe_last(&self) -> Result<Option<T>, RecvError> {
        if self.is_empty() {
            Ok(None)
        } else {
            Ok(Some(self.recv_last().await?))
        }
    }

    async fn recv_or_panic_after_timeout(&self, timeout: Duration) -> Result<T, RecvError> {
        self.recv()
            .or(async {
                Timer::after(timeout).await;
                panic!("Did not receive initial value in time");
            })
            .await
    }
}

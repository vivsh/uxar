

#[derive(Clone)]
pub struct CancellationNotifier(tokio_util::sync::CancellationToken);

impl CancellationNotifier {
    
    pub fn new() -> Self {
        Self(tokio_util::sync::CancellationToken::new())
    }

    pub fn child(&self) -> Self {
        Self(self.0.child_token())
    }

    pub(crate) fn notify_waiters(&self) {
        self.0.cancel();
    }

    pub async fn notified(&self) {
        self.0.cancelled().await;
    }
}

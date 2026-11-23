//! Discovery activity follows the external discovery task's lifetime.
#[derive(Default)]
pub(crate) struct DiscoverySession {
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl DiscoverySession {
    pub fn start(&mut self, handle: tokio::task::JoinHandle<()>) {
        self.stop();
        self.handle = Some(handle);
    }
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
    pub fn is_active(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn completed_discovery_stops_reporting_scanning_and_can_restart() {
        let mut session = DiscoverySession::default();
        session.start(tokio::spawn(async {}));
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !session.handle.as_ref().unwrap().is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            !session.is_active(),
            "completed discovery still reports scanning"
        );
        session.start(tokio::spawn(std::future::pending()));
        assert!(session.is_active());
        session.stop();
        assert!(!session.is_active());
    }
}

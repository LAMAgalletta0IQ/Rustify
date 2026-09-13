//! One shared classifier for the dealer-subscription bootstrap race.
//!
//! `DealerManager::start()` takes librespot's internal `Builder` and launches
//! the websocket asynchronously; while that future is in flight,
//! `Session::dealer().add_listen_for(...)` fails with "Builder wasn't
//! available". `remote_state::spawn`, `friends::spawn`, and
//! `jams_bridge`'s `subscribe_with_retry` each hit this independently right
//! after `Spirc::new` — the Spirc task has been spawned but hasn't finished
//! `dealer.start()` yet — and each retried it with the same string check
//! before this module existed. Kept here, rather than in any one of those
//! three, because the race belongs to the dealer itself, not to any
//! particular subscriber of it.

/// Whether `error` is the transient "dealer not ready yet" race described
/// above, as opposed to a real subscription failure.
pub(crate) fn is_builder_not_available(error: &librespot::core::Error) -> bool {
    error.to_string().contains("Builder wasn't available")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_transient_builder_race() {
        let transient = librespot::core::Error::failed_precondition("Builder wasn't available");
        assert!(is_builder_not_available(&transient));

        let unrelated = librespot::core::Error::unavailable("connection reset by peer");
        assert!(!is_builder_not_available(&unrelated));
    }
}

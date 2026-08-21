use std::sync::{Arc, Mutex};

/// Supplies the OAuth bearer token that identifies the user to Spotify.
///
/// This trait is the single seam where the Jams module meets the host app's
/// session: everything else in this module treats a token as an opaque string.
/// The host updates it on login/refresh; the module never mints one itself.
#[allow(unused_variables)]
pub trait TokenProvider: Send + Sync {
    /// Returns the current access token, or `None` if the session is not
    /// authenticated. `None` surfaces as [`super::error::JamError::ClientTokenExpired`]
    /// rather than being silently ignored.
    fn access_token(&self) -> Option<String>;
}

/// A boxed, update-in-place token that the host can swap out whenever OAuth
/// refreshes. `Default` yields an unauthenticated (`None`) token, which is
/// handy for running the module before any login has happened.
#[derive(Clone, Default)]
pub struct AccessToken(Arc<Mutex<Option<String>>>);

impl AccessToken {
    pub fn new(token: impl Into<String>) -> Self {
        let this = Self::default();
        this.set(token);
        this
    }

    pub fn set(&self, token: impl Into<String>) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = Some(token.into());
        }
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = None;
        }
    }
}

impl TokenProvider for AccessToken {
    fn access_token(&self) -> Option<String> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
}
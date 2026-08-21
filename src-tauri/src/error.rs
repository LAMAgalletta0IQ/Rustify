use serde::Serialize;

/// Everything a Tauri command can fail with.
///
/// Serialises as `{ kind, message }` so the frontend can branch on `kind`
/// (notably `PremiumRequired`) instead of string-matching messages.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not logged in.")]
    NotLoggedIn,

    #[error("A Spotify Premium subscription is required. This account's plan is \"{0}\". librespot cannot play the free, ad-supported tier.")]
    PremiumRequired(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Playback error: {0}")]
    Playback(String),

    #[error("Spotify Web API error: {0}")]
    WebApi(String),

    #[error("The Spotify session has expired. Sign in again to continue.")]
    SessionExpired,

    #[error("Spotify rejected this request (400): {0}")]
    BadRequest(String),

    /// Spotify returned HTTP 403.
    ///
    /// Not always a scope problem: Spotify refuses some endpoints outright
    /// regardless of the token. Observed live 2026-08-17 — the whole
    /// `*/contains` family answers 403 while the sibling collection endpoints
    /// (`/me/tracks`, `/me/albums`) succeed on the same token. Callers that
    /// only need decoration should degrade rather than surface this.
    #[error("Spotify refused this request (403): {0}")]
    Forbidden(String),

    #[error("This item or endpoint is unavailable on Spotify (404): {0}")]
    Unavailable(String),

    #[error("Spotify is temporarily unavailable ({status}). Try again shortly.")]
    ServiceUnavailable { status: u16 },

    /// Spotify returned HTTP 429.
    ///
    /// Usually *not* caused by this app's own request volume: librespot's
    /// default client ID is shared by every librespot-based client, so the
    /// quota is consumed globally by other people's apps.
    #[error("Spotify is rate limiting this client{}.", match .retry_after {
        Some(s) => format!(" — retry in about {s}s"),
        None => String::new(),
    })]
    RateLimited { retry_after: Option<u64> },

    #[error("{0}")]
    Other(String),

    /// A feature that only exists via Spotify's private, unofficial internal
    /// APIs (Jams, queue reorder/remove, Blend creation, ...). Never reachable
    /// through the public Web API or a supported librespot API, so it is not
    /// implemented rather than half-implemented against something unstable.
    #[error("{0} is not available: {1}")]
    FeatureUnsupported(String, String),

    /// The public Web API used to support this, but Spotify has since removed
    /// or restricted it (e.g. `/artists/{{id}}/top-tracks`, Feb 2026). Distinct
    /// from `FeatureUnsupported`, which never had a public endpoint at all.
    #[error("{0}")]
    PublicApiLimitation(String),

    /// A specific documented endpoint has no reachable equivalent for this
    /// request shape right now (e.g. it requires a private endpoint, or a
    /// scope/product tier this account lacks).
    #[error("{0}")]
    EndpointNotAvailable(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotLoggedIn => "NotLoggedIn",
            Self::PremiumRequired(_) => "PremiumRequired",
            Self::Auth(_) => "Auth",
            Self::Playback(_) => "Playback",
            Self::WebApi(_) => "WebApi",
            Self::SessionExpired => "SessionExpired",
            Self::BadRequest(_) => "BadRequest",
            Self::Forbidden(_) => "Forbidden",
            Self::Unavailable(_) => "Unavailable",
            Self::ServiceUnavailable { .. } => "ServiceUnavailable",
            Self::RateLimited { .. } => "RateLimited",
            Self::Other(_) => "Other",
            Self::FeatureUnsupported(..) => "FeatureUnsupported",
            Self::PublicApiLimitation(_) => "PublicApiLimitation",
            Self::EndpointNotAvailable(_) => "EndpointNotAvailable",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 3)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.to_string())?;
        // Exposed as its own field so the UI can run a countdown rather than
        // parsing the number back out of the message text.
        st.serialize_field(
            "retryAfter",
            &match self {
                Self::RateLimited { retry_after } => *retry_after,
                _ => None,
            },
        )?;
        st.end()
    }
}

// librespot's own error type is opaque enough that mapping it wholesale to
// Playback would lose the auth/network distinction. Callers pick the variant.
impl From<librespot::core::Error> for AppError {
    fn from(e: librespot::core::Error) -> Self {
        Self::Playback(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        Self::WebApi(e.to_string())
    }
}

/// Jams uses its own error taxonomy for the internal services (client token,
/// persisted queries, dealer). Map it onto the app's kinds so the UI can branch
/// on them the same way it does for every other command.
impl From<crate::jams::JamError> for AppError {
    fn from(e: crate::jams::JamError) -> Self {
        match e {
            crate::jams::JamError::JamNotFound(m) => Self::Unavailable(m),
            crate::jams::JamError::PermissionDenied(m) => Self::Forbidden(m),
            crate::jams::JamError::ClientTokenExpired(m) => Self::Auth(m),
            crate::jams::JamError::Config(m) => Self::BadRequest(m),
            other => Self::Other(other.to_string()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Other(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

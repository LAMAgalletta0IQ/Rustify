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

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotLoggedIn => "NotLoggedIn",
            Self::PremiumRequired(_) => "PremiumRequired",
            Self::Auth(_) => "Auth",
            Self::Playback(_) => "Playback",
            Self::WebApi(_) => "WebApi",
            Self::Other(_) => "Other",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.to_string())?;
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

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Other(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

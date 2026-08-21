//! Spotify first-party service integrations that are shared by user-facing
//! features. These clients use credentials from the authenticated librespot
//! session and never persist or log them.

mod dj;
mod home;
mod pathfinder;

pub use dj::DjSession;
pub use home::HomeFeed;

use librespot::core::session::Session;

use crate::error::{AppError, AppResult};

#[derive(Default)]
pub struct InternalSpotify {
    pub(crate) dj: dj::DjClient,
    pathfinder: pathfinder::PathfinderClient,
}

struct FirstPartyAuth {
    access_token: String,
    client_token: String,
    connection_id: String,
}

async fn first_party_auth(session: &Session) -> AppResult<FirstPartyAuth> {
    Ok(FirstPartyAuth {
        access_token: session
            .login5()
            .auth_token()
            .await
            .map_err(|error| AppError::Auth(format!("first-party token: {error}")))?
            .access_token,
        client_token: session
            .spclient()
            .client_token()
            .await
            .map_err(|error| AppError::Auth(format!("client-token: {error}")))?,
        connection_id: session.connection_id(),
    })
}

impl InternalSpotify {
    pub async fn home(
        &self,
        session: &Session,
        limit: u32,
        time_zone: &str,
    ) -> AppResult<HomeFeed> {
        let auth = first_party_auth(session).await?;

        let variables = home::home_variables(limit.clamp(1, 20), time_zone);
        let data = self
            .pathfinder
            .query(
                "home",
                variables,
                &auth.access_token,
                &auth.client_token,
                &auth.connection_id,
            )
            .await?;
        home::parse_home(&data)
    }

    pub async fn resolve_dj(&self, session: &Session, restore: bool) -> AppResult<DjSession> {
        let auth = first_party_auth(session).await?;
        self.dj
            .resolve(
                session
                    .spclient()
                    .base_url()
                    .await
                    .map_err(AppError::from)?,
                if restore {
                    "state_restore"
                } else {
                    "interactive"
                },
                &auth.access_token,
                &auth.client_token,
                &auth.connection_id,
            )
            .await
    }

    pub async fn cached_dj(&self) -> Option<DjSession> {
        self.dj.cached().await
    }

    pub async fn prepare_dj_narration(&self, session: &Session, dj: &DjSession) -> AppResult<bool> {
        let auth = first_party_auth(session).await?;
        self.dj
            .prepare_first_narration(
                session
                    .spclient()
                    .base_url()
                    .await
                    .map_err(AppError::from)?,
                dj,
                &auth.access_token,
                &auth.client_token,
                &auth.connection_id,
            )
            .await
    }
}

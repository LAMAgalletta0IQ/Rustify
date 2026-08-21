//! Spotify first-party service integrations that are shared by user-facing
//! features. These clients use credentials from the authenticated librespot
//! session and never persist or log them.

mod home;
mod pathfinder;

pub use home::HomeFeed;

use librespot::core::session::Session;

use crate::error::{AppError, AppResult};

#[derive(Default)]
pub struct InternalSpotify {
    pathfinder: pathfinder::PathfinderClient,
}

impl InternalSpotify {
    pub async fn home(
        &self,
        session: &Session,
        limit: u32,
        time_zone: &str,
    ) -> AppResult<HomeFeed> {
        let access_token = session
            .login5()
            .auth_token()
            .await
            .map_err(|error| AppError::Auth(format!("first-party token: {error}")))?
            .access_token;
        let client_token = session
            .spclient()
            .client_token()
            .await
            .map_err(|error| AppError::Auth(format!("client-token: {error}")))?;

        let variables = home::home_variables(limit.clamp(1, 20), time_zone);
        let data = self
            .pathfinder
            .query(
                "home",
                variables,
                &access_token,
                &client_token,
                session.connection_id().as_str(),
            )
            .await?;
        home::parse_home(&data)
    }
}

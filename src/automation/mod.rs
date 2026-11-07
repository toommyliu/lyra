pub(crate) mod auth;
pub(crate) mod bridge;
pub(crate) mod combat;
pub(crate) mod scripting;
pub(crate) mod settings;
#[cfg(test)]
mod testing;
pub(crate) mod world;

use std::time::Duration;

use tokio::time::{Instant, sleep};

use bridge::CallError;

async fn poll<T>(
    every: Duration,
    within: Duration,
    mut check: impl FnMut() -> Result<Option<T>, CallError>,
) -> Result<Option<T>, CallError> {
    let deadline = Instant::now() + within;
    loop {
        if let Some(value) = check()? {
            return Ok(Some(value));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        sleep(every.min(deadline - now)).await;
    }
}

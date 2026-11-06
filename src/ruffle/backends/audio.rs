use ruffle_core::backend::audio::{AudioBackend, NullAudioBackend};
use ruffle_frontend_utils::backends::audio::CpalAudioBackend;

pub(crate) fn audio() -> Box<dyn AudioBackend> {
    match CpalAudioBackend::new(None) {
        Ok(audio) => Box::new(audio),
        Err(error) => {
            tracing::warn!(%error, "no audio output, the player runs silent");
            Box::new(NullAudioBackend::new())
        }
    }
}

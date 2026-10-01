//! WebRTC AEC3 at 48 kHz / 10 ms. Feed audio actually rendered by the
//! hardware, including underrun silence; never use received network packets.
use webrtc_audio_processing::{Processor, Config, config::{EchoCanceller, HighPassFilter}};
use crate::AudioError;

pub const DSP_SAMPLES: usize = 480;
pub struct EchoProcessor { processor: Processor }
impl EchoProcessor {
    pub fn new() -> Result<Self, AudioError> {
        let processor = Processor::new(48_000).map_err(|_|AudioError::Device)?;
        processor.set_config(Config { echo_canceller:Some(EchoCanceller::Full { stream_delay_ms:None }), high_pass_filter:Some(HighPassFilter::default()), ..Config::default() });
        Ok(Self { processor })
    }
    pub fn render(&self, actually_played: &[f32; DSP_SAMPLES]) -> Result<(), AudioError> {
        self.processor.analyze_render_frame([actually_played.as_slice()]).map_err(|_|AudioError::Frame)
    }
    pub fn capture(&self, microphone: &mut [f32; DSP_SAMPLES]) -> Result<(), AudioError> {
        self.processor.process_capture_frame([microphone.as_mut_slice()]).map_err(|_|AudioError::Frame)
    }
    /// Device/session changes discard adaptive history as well as samples.
    pub fn reset(&mut self) -> Result<(),AudioError> { *self=Self::new()?; Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reverse_and_capture_frames_are_finite_and_reset_cleanly() {
        let mut processor=EchoProcessor::new().unwrap();
        for frame in 0..400 {
            let mut reference=[0.0;DSP_SAMPLES];
            for (i,v) in reference.iter_mut().enumerate() { *v=((frame*DSP_SAMPLES+i) as f32*0.04).sin()*0.2; }
            processor.render(&reference).unwrap();
            let mut microphone=reference.map(|s|s*0.6);
            processor.capture(&mut microphone).unwrap();
            assert!(microphone.iter().all(|s|s.is_finite()));
        }
        processor.reset().unwrap();
        processor.capture(&mut [0.0;DSP_SAMPLES]).unwrap();
    }
}

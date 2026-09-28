//! Opt-in real-model smoke test; normal unit tests never download weights.
use std::{io::Cursor, path::PathBuf, sync::atomic::AtomicBool};

#[test]
#[ignore = "downloads the pinned 670 MB model; run explicitly in platform CI"]
fn pinned_model_transcribes_device_rate_speech() -> anyhow::Result<()> {
    let dir = PathBuf::from(
        std::env::var_os("ZERON_VOICE_TEST_MODEL_DIR")
            .expect("set ZERON_VOICE_TEST_MODEL_DIR to an isolated model cache"),
    );
    if !zeron_voice::installed(&dir) {
        zeron_voice::download(&dir, &AtomicBool::new(false), |_| {})?;
    }
    let mut model = zeron_voice::Recognizer::load(&dir)?;
    for (bytes, expected) in [
        (
            include_bytes!("fixtures/english-48000.wav").as_slice(),
            "please update the rust function",
        ),
        (
            include_bytes!("fixtures/french-44100.wav").as_slice(),
            "modifier la fonction",
        ),
    ] {
        let mut wav = hound::WavReader::new(Cursor::new(bytes))?;
        let spec = wav.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bits_per_sample, 16);
        let samples = wav
            .samples::<i16>()
            .map(|s| s.map(|s| s as f32 / 32768.0))
            .collect::<Result<Vec<_>, _>>()?;
        let text = model.transcribe(samples, spec.sample_rate)?;
        // These are synthetic public test phrases, never microphone recordings.
        println!("rate={} transcript={text:?}", spec.sample_rate);
        assert!(
            text.to_lowercase().contains(expected),
            "unexpected transcript: {text:?}"
        );
    }
    assert!(model.transcribe(vec![0.0; 48_000], 48_000)?.is_empty());
    Ok(())
}

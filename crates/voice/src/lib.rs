//! Desktop-local Parakeet v3. No engine, document, RPC or audio persistence.
use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parakeet_rs::{ParakeetTDT, Transcriber};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

mod resample;

pub const MAX_SECONDS: usize = 60;
#[derive(serde::Deserialize)]
pub struct Artifact {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(serde::Deserialize)]
pub struct Manifest {
    pub repository: String,
    pub revision: String,
    pub files: Vec<Artifact>,
}
pub fn manifest() -> Manifest {
    serde_json::from_str(include_str!("../model.json")).expect("pinned model manifest")
}
pub fn download_size() -> u64 {
    manifest().files.iter().map(|f| f.size).sum()
}
pub fn installed(dir: &Path) -> bool {
    manifest()
        .files
        .iter()
        .all(|f| std::fs::metadata(dir.join(&f.name)).is_ok_and(|m| m.len() == f.size))
        && std::fs::read_to_string(dir.join("verified")).is_ok_and(|r| r == manifest().revision)
}

/// Called only on a worker. Temporary files never establish readiness.
pub fn download(dir: &Path, cancel: &AtomicBool, mut progress: impl FnMut(u64)) -> Result<()> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        std::fs::create_dir_all(dir)?;
        let client = reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(20)).build()?;
        let m=manifest(); let mut total=0;
        for file in m.files {
            if cancel.load(Ordering::Acquire) { bail!("Download cancelled") }
            let staging=dir.join(format!("{}.part",file.name));
            let result=async {
                let mut response=tokio::select! {
                    _=cancelled(cancel)=>bail!("Download cancelled"),
                    response=client.get(format!("https://huggingface.co/{}/resolve/{}/{}",m.repository,m.revision,file.name)).send()=>response?.error_for_status()?,
                };
                let mut out=std::fs::File::create(&staging)?;let mut hash=Sha256::new();let mut size=0;
                loop {
                    let chunk=tokio::select! {
                        _=cancelled(cancel)=>bail!("Download cancelled"),
                        chunk=tokio::time::timeout(std::time::Duration::from_secs(30),response.chunk())=>chunk??,
                    };
                    let Some(chunk)=chunk else {break};
                    size+=chunk.len() as u64;if size>file.size {bail!("Unexpected model size")}
                    hash.update(&chunk);out.write_all(&chunk)?;total+=chunk.len() as u64;progress(total);
                }
                if size!=file.size || format!("{:x}",hash.finalize())!=file.sha256 {bail!("Model checksum verification failed")}
                out.sync_all()?;std::fs::rename(&staging,dir.join(&file.name))?;Ok::<_,anyhow::Error>(())
            }.await;
            if result.is_err() {let _=std::fs::remove_file(staging);} result?;
        }
        if cancel.load(Ordering::Acquire) {bail!("Download cancelled")}
        std::fs::write(dir.join("verified"),manifest().revision)?;Ok(())
    })
}
async fn cancelled(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Acquire) {
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
}

pub struct Recognizer(ParakeetTDT);
impl Recognizer {
    pub fn load(dir: &Path) -> Result<Self> {
        // Verify before handing bytes to the native runtime, including after restart.
        for f in manifest().files {
            let mut input = std::fs::File::open(dir.join(&f.name))?;
            let mut hash = Sha256::new();
            std::io::copy(&mut input, &mut hash)?;
            if format!("{:x}", hash.finalize()) != f.sha256 {
                bail!("Model is damaged. Remove it in Settings and download again.")
            }
        }
        Ok(Self(ParakeetTDT::from_pretrained(dir, None).map_err(
            |_| anyhow::anyhow!("Could not load Parakeet v3"),
        )?))
    }
    pub fn transcribe(&mut self, samples: Vec<f32>, rate: u32) -> Result<String> {
        if !(8_000..=192_000).contains(&rate) {
            bail!("Unsupported microphone sample rate")
        }
        if samples.len() > rate as usize * MAX_SECONDS {
            bail!("Recording exceeds one minute")
        }
        if samples.len() < rate as usize / 5 || samples.iter().all(|s| s.abs() < 0.0001) {
            return Ok(String::new());
        }
        let samples = resample::for_model(samples, rate)?;
        self.0
            .transcribe_samples(samples, resample::MODEL_RATE, 1, None)
            .map(|r| r.text)
            .map_err(|_| anyhow::anyhow!("Could not transcribe this recording"))
    }
}

struct Audio {
    samples: Vec<f32>,
    failed: bool,
    full: bool,
}
fn append<T: cpal::Sample>(data: &[T], channels: usize, rate: u32, a: &Mutex<Audio>)
where
    f32: cpal::FromSample<T>,
{
    if let Ok(mut a) = a.try_lock() {
        for frame in data.chunks_exact(channels) {
            if a.samples.len() >= rate as usize * MAX_SECONDS {
                a.full = true;
                break;
            }
            a.samples
                .push(frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32);
        }
    }
}
/// A microphone the user can choose: a stable identifier and a display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDevice {
    pub id: String,
    pub name: String,
}

/// Every input device the default host reports. Enumeration can block on the
/// audio server, so call it off the UI thread.
pub fn input_devices() -> Vec<InputDevice> {
    let Ok(devices) = cpal::default_host().input_devices() else {
        return Vec::new();
    };
    let mut devices: Vec<_> = devices
        .filter_map(|device| {
            Some(InputDevice {
                id: device.id().ok()?.to_string(),
                name: device.description().ok()?.name().to_owned(),
            })
        })
        .collect();
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

/// The id of the device the system currently records from by default.
pub fn default_input_device() -> Option<String> {
    Some(
        cpal::default_host()
            .default_input_device()?
            .id()
            .ok()?
            .to_string(),
    )
}

/// The chosen device when it is connected, the system default otherwise.
fn input_device(id: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    id.and_then(|id| id.parse::<cpal::DeviceId>().ok())
        .and_then(|id| host.device_by_id(&id))
        .or_else(|| host.default_input_device())
}

pub struct Capture {
    stream: Option<cpal::Stream>,
    audio: Arc<Mutex<Audio>>,
    rate: u32,
}
impl Capture {
    pub fn start(device: Option<&str>) -> Result<Self> {
        let device = input_device(device).context("No microphone available")?;
        let config = device.default_input_config()?;
        let rate = config.sample_rate();
        let channels = config.channels() as usize;
        let audio = Arc::new(Mutex::new(Audio {
            samples: Vec::with_capacity(rate as usize * MAX_SECONDS),
            failed: false,
            full: false,
        }));
        let a = audio.clone();
        let e = audio.clone();
        let err = move |_| {
            if let Ok(mut a) = e.lock() {
                a.failed = true;
            }
        };
        // Preserve the device's native configuration; convert every CPAL
        // sample representation through the same bounded mono callback.
        macro_rules! stream {
            ($sample:ty) => {
                device.build_input_stream(
                    &config.into(),
                    move |d: &[$sample], _| append(d, channels, rate, &a),
                    err,
                    None,
                )?
            };
        }
        let stream = match config.sample_format() {
            cpal::SampleFormat::I8 => stream!(i8),
            cpal::SampleFormat::I16 => stream!(i16),
            cpal::SampleFormat::I24 => stream!(cpal::I24),
            cpal::SampleFormat::I32 => stream!(i32),
            cpal::SampleFormat::I64 => stream!(i64),
            cpal::SampleFormat::U8 => stream!(u8),
            cpal::SampleFormat::U16 => stream!(u16),
            cpal::SampleFormat::U24 => stream!(cpal::U24),
            cpal::SampleFormat::U32 => stream!(u32),
            cpal::SampleFormat::U64 => stream!(u64),
            cpal::SampleFormat::F32 => stream!(f32),
            cpal::SampleFormat::F64 => stream!(f64),
            _ => bail!("Unsupported microphone format"),
        };
        stream.play()?;
        Ok(Self {
            stream: Some(stream),
            audio,
            rate,
        })
    }
    pub fn ended(&self) -> bool {
        self.audio.lock().map_or(true, |a| a.full || a.failed)
    }
    pub fn finish(mut self) -> Result<(Vec<f32>, u32)> {
        self.stream.take();
        let mut audio = self.audio.lock().unwrap();
        if audio.failed {
            bail!("Microphone disconnected. Your draft is safe.")
        }
        Ok((std::mem::take(&mut audio.samples), self.rate))
    }
}

#[derive(Debug)]
pub enum Event {
    Listening,
    Finalizing,
    Final(String),
    Failed(String),
}
struct Job {
    dir: std::path::PathBuf,
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    device: Option<String>,
    events: std::sync::mpsc::SyncSender<Event>,
}
static BUSY: AtomicBool = AtomicBool::new(false);
enum Command {
    Transcribe(Job),
    Unload,
}
static WORKER: std::sync::OnceLock<std::sync::mpsc::SyncSender<Command>> =
    std::sync::OnceLock::new();
pub struct Session {
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    events: std::sync::mpsc::Receiver<Event>,
}
impl Session {
    /// `device` is an [`InputDevice::id`]; `None` or a disconnected device
    /// records from the system default.
    pub fn start(dir: std::path::PathBuf, device: Option<String>) -> Result<Self> {
        if BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            bail!("Dictation is still active or finishing. Wait a moment, then try again.")
        }
        let sender = WORKER.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::sync_channel::<Command>(1);
            std::thread::spawn(move || {
                let mut cached: Option<(std::path::PathBuf, Recognizer)> = None;
                loop {
                    let job = match rx.recv_timeout(std::time::Duration::from_secs(30)) {
                        Ok(Command::Transcribe(j)) => j,
                        Ok(Command::Unload) => {
                            cached = None;
                            continue;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            cached = None;
                            continue;
                        }
                        Err(_) => break,
                    };
                    let result = (|| -> Result<()> {
                        if job.cancel.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        if cached.as_ref().is_none_or(|(path, _)| *path != job.dir) {
                            cached = Some((job.dir.clone(), Recognizer::load(&job.dir).context("Could not load the model. Remove it in Settings and download again.")?));
                        }
                        if job.cancel.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        if job.stop.load(Ordering::Acquire) {
                            let _ = job.events.try_send(Event::Final(String::new()));
                            return Ok(());
                        }
                        let capture = Capture::start(job.device.as_deref())?;
                        let _ = job.events.try_send(Event::Listening);
                        let start = std::time::Instant::now();
                        while !job.stop.load(Ordering::Acquire)
                            && !job.cancel.load(Ordering::Acquire)
                            && !capture.ended()
                            && start.elapsed().as_secs() < MAX_SECONDS as u64
                        {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        if job.cancel.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        let (samples, rate) = capture.finish()?;
                        let _ = job.events.try_send(Event::Finalizing);
                        let text = cached.as_mut().unwrap().1.transcribe(samples, rate)?;
                        if !job.cancel.load(Ordering::Acquire) {
                            let _ = job.events.try_send(Event::Final(text));
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        cached = None;
                        let _ = job.events.try_send(Event::Failed(e.to_string()));
                    }
                    BUSY.store(false, Ordering::Release);
                }
            });
            tx
        });
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, events) = std::sync::mpsc::sync_channel(4);
        if sender
            .try_send(Command::Transcribe(Job {
                dir,
                stop: stop.clone(),
                cancel: cancel.clone(),
                device,
                events: tx,
            }))
            .is_err()
        {
            BUSY.store(false, Ordering::Release);
            bail!("Dictation worker unavailable")
        }
        Ok(Self {
            stop,
            cancel,
            events,
        })
    }
    pub fn poll(&mut self) -> Option<Event> {
        self.events.try_recv().ok()
    }
    pub fn finish(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
pub fn busy() -> bool {
    BUSY.load(Ordering::Acquire)
}

/// Retire cached weights on the worker, never on the UI thread.
pub fn unload() {
    if let Some(worker) = WORKER.get() {
        let _ = worker.try_send(Command::Unload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::Sample;
    #[test]
    fn native_sample_formats_preserve_levels_and_downmix_channels() {
        fn check<T: cpal::Sample + cpal::FromSample<f32>>()
        where
            f32: cpal::FromSample<T>,
        {
            let audio = Mutex::new(Audio {
                samples: Vec::new(),
                failed: false,
                full: false,
            });
            let input: Vec<T> = [-0.5_f32, 0.5, 0.25, 0.75, 0.0, 0.0]
                .into_iter()
                .map(|s| s.to_sample::<T>())
                .collect();
            append(&input, 2, 16_000, &audio);
            assert_eq!(audio.lock().unwrap().samples, vec![0.0, 0.5, 0.0]);
        }
        check::<i8>();
        check::<i16>();
        check::<cpal::I24>();
        check::<i32>();
        check::<i64>();
        check::<u8>();
        check::<u16>();
        check::<cpal::U24>();
        check::<u32>();
        check::<u64>();
        check::<f32>();
        check::<f64>();
    }
    #[test]
    fn manifest_pins_only_v3_artifacts_and_exact_size() {
        let m = manifest();
        assert_eq!(m.revision.len(), 40);
        assert_eq!(m.files.len(), 3);
        assert_eq!(download_size(), 670479942);
        assert!(
            m.files
                .iter()
                .all(|f| f.sha256.len() == 64 && !f.name.contains('/'))
        );
    }
    #[test]
    fn cancelled_download_never_establishes_readiness() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            download(dir.path(), &AtomicBool::new(true), |_| panic!(
                "no progress after cancellation"
            ))
            .is_err()
        );
        assert!(!installed(dir.path()));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    #[test]
    fn partial_or_corrupt_model_cannot_load() {
        let dir = tempfile::tempdir().unwrap();
        let f = &manifest().files[0];
        std::fs::write(dir.path().join(&f.name), b"corrupt").unwrap();
        assert!(!installed(dir.path()));
        assert!(Recognizer::load(dir.path()).is_err());
    }
}

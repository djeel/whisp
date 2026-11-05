//! Audio capture for whisp.
//!
//! PipeWire native capture goes through the PulseAudio compat layer, which
//! `cpal` reaches via its ALSA backend (`default_input_device`). Samples are
//! downmixed to mono, statefully resampled to 16 kHz, and pushed into a
//! heap ringbuffer (`HeapRb<f32>`, capacity = 10 min). A throttled RMS level
//! (10 Hz max) is emitted as `whisp://level` for the recorder waveform.
//!
//! The `cpal::Stream` is `!Send`, so it lives on a dedicated capture thread
//! and never crosses into Tauri-managed state. [`RecordingHandle`] (all
//! `Send`) is what the rest of the app holds; dropping it signals the
//! thread to stop.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{traits::*, HeapRb};
use tauri::Emitter;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;
/// 10 minutes of 16 kHz mono f32.
pub const RING_CAPACITY: usize = 16_000 * 600;
/// Minimum level-emit interval (10 Hz max).
const LEVEL_MIN_INTERVAL: Duration = Duration::from_millis(100);

// ---------------------------------------------------------------------------
// Level / VAD helpers (pure, unit-tested)
// ---------------------------------------------------------------------------

/// RMS level of a mono f32 frame, normalised to 0.0..1.0 (clamped).
pub fn rms_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

/// Simple energy VAD with hangover.
///
/// A frame is speech when its RMS exceeds `threshold`; once triggered,
/// speech is held for `hangover` consecutive silent frames so word tails
/// are not clipped. Default hangover = 400 ms of 20 ms frames (20 frames).
pub struct Vad {
    threshold: f32,
    hangover_frames: u32,
    silent_left: u32,
}

impl Vad {
    pub fn new(threshold: f32, hangover_frames: u32) -> Self {
        Self {
            threshold,
            hangover_frames,
            silent_left: 0,
        }
    }

    pub fn default_16k() -> Self {
        Self::new(0.02, 20)
    }

    /// Feed one frame (e.g. 20 ms = 320 samples @16 kHz). Returns true while
    /// speech (or hangover) is active.
    pub fn is_speech(&mut self, frame: &[f32]) -> bool {
        if rms_level(frame) > self.threshold {
            self.silent_left = self.hangover_frames;
            true
        } else if self.silent_left > 0 {
            self.silent_left -= 1;
            true
        } else {
            false
        }
    }
}

/// Linear-interpolating resampler: device-rate mono -> 16 kHz mono.
/// Carried across audio callbacks so chunk boundaries do not click.
/// (Zero-order-hold aliasing was degrading STT; linear interp fixes it.)
pub struct ChunkResampler {
    step: f64,
    pos: f64,
    prev: f32,
    has_prev: bool,
}

impl ChunkResampler {
    pub fn new(from_rate: u32) -> Self {
        Self {
            step: TARGET_SAMPLE_RATE as f64 / from_rate.max(1) as f64,
            pos: 0.0,
            prev: 0.0,
            has_prev: false,
        }
    }

    pub fn push_chunk(&mut self, mono: &[f32], out: &mut Vec<f32>) {
        for &s in mono {
            if !self.has_prev {
                self.prev = s;
                self.has_prev = true;
            }
            self.pos += self.step;
            while self.pos >= 1.0 {
                // frac=1-pos..1 : interpolate prev -> current
                let frac = (1.0 - self.pos).clamp(0.0, 1.0) as f32;
                out.push(self.prev + (s - self.prev) * frac);
                self.pos -= 1.0;
            }
            self.prev = s;
        }
    }
}
/// Copy at most `max` newest samples from `ring` (oldest first). Bounded:
/// holds the ring lock for milliseconds, never for the whole 10-minute
/// ring. `usize::MAX` = everything (legacy `snapshot` path).
fn snapshot_ring_last(ring: &Mutex<HeapRb<f32>>, max: usize) -> Vec<f32> {
    let ring = ring.lock().unwrap_or_else(|e| e.into_inner());
    let (a, b) = ring.as_slices();
    let total = a.len() + b.len();
    let skip = total.saturating_sub(max);
    let mut out = Vec::with_capacity(total.min(max));
    for &s in a.iter().chain(b.iter()).skip(skip) {
        out.push(s);
    }
    out
}

/// Downmix interleaved f32 samples to mono (pure, testée ; chemins froids).
/// Le hot path (callback cpal) fait le downmix inline dans les scratch
/// `thread_local` réutilisés — zéro allocation par paquet audio.
pub fn downmix_f32(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|ch| ch.iter().sum::<f32>() / ch.len() as f32)
        .collect()
}

// ---------------------------------------------------------------------------
// Live capture: stream lives on a worker thread, handle is Send
// ---------------------------------------------------------------------------

/// `Send`-safe handle to an active capture. Dropping it stops the device.
pub struct RecordingHandle {
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    pub ring: Arc<Mutex<HeapRb<f32>>>,
    pub from_rate: u32,
    started_at: Instant,
}

impl Drop for RecordingHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl RecordingHandle {
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Drain everything captured so far (oldest first) into a Vec.
    /// `unwrap_or_else(into_inner)` : un panic passé ne doit jamais geler
    /// les dictées suivantes (poison = figeait tout avant).
    pub fn drain(&self) -> Vec<f32> {
        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::with_capacity(ring.occupied_len());
        while let Some(s) = ring.try_pop() {
            out.push(s);
        }
        out
    }
    /// Oldest first. Ring keeps growing; the final `drain`/`finish` still
    /// gets the full audio.
    pub fn snapshot(&self) -> Vec<f32> {
        snapshot_ring_last(&self.ring, usize::MAX)
    }

    /// Copy at most `max` newest samples WITHOUT draining (for live previews).
    /// Bounded: the preview only needs seconds, never the full 10-minute
    /// ring, so the ring lock is held briefly and `stop` never waits on it.
    pub fn snapshot_last(&self, max: usize) -> Vec<f32> {
        snapshot_ring_last(&self.ring, max)
    }

    /// Signal the capture thread to stop, flush, then drain.
    pub fn finish(mut self) -> Vec<f32> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        // Let the audio thread push its final callback(s).
        std::thread::sleep(Duration::from_millis(100));
        self.drain()
    }
}

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devs| devs.filter_map(|d| d.name().ok()).collect::<Vec<_>>())
        .unwrap_or_default()
}
/// Preferred device from config, else system default. Names come from
/// `list_input_devices` (cpal device names, e.g. PipeWire node names).
pub fn pick_input_device(host: &cpal::Host) -> Result<cpal::Device, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let wanted = crate::config::input_device_name();
    if let Some(name) = wanted {
        if let Ok(devs) = host.input_devices() {
            for d in devs {
                if d.name().is_ok_and(|n| n == name) {
                    return Ok(d);
                }
            }
        }
        // Configured device unplugged: fall back to default (don't fail dictation).
        eprintln!("[whisp] input '{name}' not found, using default");
    }
    host.default_input_device()
        .ok_or_else(|| "no default input device (PipeWire/Pulse running?)".to_string())
}

/// Spawn the capture thread and wait (max 5 s) for the device to be ready.
/// Resampled 16 kHz mono is pushed into the handle's ring; RMS levels are
/// emitted on `whisp://level` at most every 100 ms when `emit_levels`.
pub fn spawn_session(app: tauri::AppHandle, emit_levels: bool) -> Result<RecordingHandle, String> {
    let ring: Arc<Mutex<HeapRb<f32>>> = Arc::new(Mutex::new(HeapRb::new(RING_CAPACITY)));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let ring_worker = Arc::clone(&ring);

    std::thread::Builder::new()
        .name("whisp-capture".to_string())
        .spawn(move || {
            let built = build_capture_stream(&app, &ring_worker, emit_levels);
            match built {
                Ok((stream, from_rate)) => match stream.play() {
                    Ok(()) => {
                        let _ = ready_tx.send(Ok(from_rate));
                        // Hold the stream until stop; dropping ends capture.
                        let _ = stop_rx.recv();
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("start input stream: {e}")));
                    }
                },
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        })
        .map_err(|e| format!("spawn capture thread: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(from_rate)) => Ok(RecordingHandle {
            stop_tx: Some(stop_tx),
            ring,
            from_rate,
            started_at: Instant::now(),
        }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("audio device did not become ready".to_string()),
    }
}

fn build_capture_stream(
    app: &tauri::AppHandle,
    ring: &Arc<Mutex<HeapRb<f32>>>,
    emit_levels: bool,
) -> Result<(cpal::Stream, u32), String> {
    let host = cpal::default_host();
    let device = pick_input_device(&host)?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default input config: {e}"))?;
    let from_rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let resampler = Arc::new(Mutex::new(ChunkResampler::new(from_rate)));
    let last_emit = Arc::new(Mutex::new(Instant::now() - LEVEL_MIN_INTERVAL * 2));

    // Scratch buffers hors callback : `thread_local!` car le callback cpal
    // s'exécute toujours sur le même thread audio. `clear()` + réutilisation
    // = zéro allocation par paquet (mesure : avant = 3 `Vec` + 1 `reserve`
    // par paquet à 50-100 Hz ; après = 0 alloc en régime établi).
    thread_local! {
        static SCRATCH_MONO: std::cell::RefCell<Vec<f32>> = const { std::cell::RefCell::new(Vec::new()) };
        static SCRATCH_OUT: std::cell::RefCell<Vec<f32>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    macro_rules! build_stream {
        ($ty:ty, $conv:expr) => {{
            let ring = Arc::clone(ring);
            let resampler = Arc::clone(&resampler);
            let last_emit = Arc::clone(&last_emit);
            let app = app.clone();
            let conv: fn($ty) -> f32 = $conv;
            device.build_input_stream(
                &config,
                move |data: &[$ty], _| {
                    SCRATCH_MONO.with(|mono_cell| {
                        SCRATCH_OUT.with(|out_cell| {
                            let mut mono = mono_cell.borrow_mut();
                            let mut out = out_cell.borrow_mut();
                            // Convert + downmix at device rate, sans allocation.
                            mono.clear();
                            if channels <= 1 {
                                mono.reserve(data.len());
                                mono.extend(data.iter().map(|&s| conv(s)));
                            } else {
                                mono.reserve(data.len() / channels.max(1) + 1);
                                for ch in data.chunks(channels) {
                                    mono.push(
                                        ch.iter().map(|&s| conv(s)).sum::<f32>() / ch.len() as f32,
                                    );
                                }
                            }
                            // Level: cheap check FIRST, lock only when emitting (the
                            // callback is real-time: never hold a mutex per packet).
                            if emit_levels {
                                let due = last_emit
                                    .lock()
                                    .map(|mut last| {
                                        if last.elapsed() >= LEVEL_MIN_INTERVAL {
                                            *last = Instant::now();
                                            true
                                        } else {
                                            false
                                        }
                                    })
                                    .unwrap_or(false);
                                if due {
                                    let _ = app.emit("whisp://level", rms_level(&mono));
                                }
                            }
                            // Resample -> 16 kHz -> ring (drop oldest on overflow).
                            // Single lock of the resampler (no double .lock() per packet).
                            out.clear();
                            if let Ok(mut r) = resampler.lock() {
                                let step = r.step;
                                out.reserve((mono.len() as f64 * step) as usize + 1);
                                r.push_chunk(&mono, &mut out);
                                drop(r);
                                if let Ok(mut ring) = ring.lock() {
                                    // `extend` par copie : `HeapRb::try_push` un
                                    // par un, même sémantique drop-oldest.
                                    for &s in out.iter() {
                                        let _ = ring.try_push(s);
                                    }
                                }
                            }
                        })
                    });
                },
                move |err| eprintln!("[whisp] audio stream error: {err}"),
                None,
            )
        }};
    }

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_stream!(f32, (|s| s) as fn(f32) -> f32),
        cpal::SampleFormat::I16 => build_stream!(i16, (|s| s as f32 / 32768.0) as fn(i16) -> f32),
        cpal::SampleFormat::U16 => {
            build_stream!(u16, (|s| (s as f32 - 32768.0) / 32768.0) as fn(u16) -> f32)
        }
        other => return Err(format!("unsupported sample format: {other:?}")),
    }
    .map_err(|e| format!("build input stream: {e}"))?;

    Ok((stream, from_rate))
}

/// Encode 16 kHz mono f32 as 16-bit PCM WAV bytes (for history retention).
/// Never panics: in-memory writes only fail on OOM, mapped to empty vec
/// so a corrupt frame never aborts the release binary (panic=abort).
pub fn encode_wav_16k_mono(pcm: &[f32]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut writer = match hound::WavWriter::new(&mut cursor, spec) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[whisp] wav writer init failed: {e}");
            return Vec::new();
        }
    };
    for &s in pcm {
        if let Err(e) = writer.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16) {
            eprintln!("[whisp] wav sample write failed: {e}");
            return Vec::new();
        }
    }
    if let Err(e) = writer.finalize() {
        eprintln!("[whisp] wav finalize failed: {e}");
        return Vec::new();
    }
    cursor.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech_frame() -> Vec<f32> {
        // 20 ms @16 kHz sine-ish burst well above threshold.
        (0..320).map(|i| 0.5 * (i as f32 * 0.2).sin()).collect()
    }

    #[test]
    fn vad_detects_speech_and_holds_hangover() {
        let mut vad = Vad::default_16k();
        let silence = vec![0.0001f32; 320];
        assert!(!vad.is_speech(&silence));
        assert!(vad.is_speech(&speech_frame()));
        // Hangover: silence right after speech still counts as speech.
        assert!(vad.is_speech(&silence));
        // After hangover frames expire, silence is silence again.
        for _ in 0..25 {
            vad.is_speech(&silence);
        }
        assert!(!vad.is_speech(&silence));
    }

    #[test]
    fn resample_48k_to_16k_mono_length() {
        // 1 s of 48 kHz mono -> ~16000 samples.
        let input = vec![0.1f32; 48_000];
        let mut r = ChunkResampler::new(48_000);
        let mut out = Vec::new();
        // Feed in uneven callback-sized chunks to prove statefulness.
        for chunk in input.chunks(1024) {
            r.push_chunk(chunk, &mut out);
        }
        assert!((out.len() as i32 - 16_000).abs() <= 2, "got {}", out.len());
        assert!(out.iter().all(|&s| (s - 0.1).abs() < 1e-6));
    }

    #[test]
    fn downmix_stereo_to_mono() {
        let stereo = vec![1.0f32, -1.0, 0.5, 0.5];
        assert_eq!(downmix_f32(&stereo, 2), vec![0.0, 0.5]);
    }
}

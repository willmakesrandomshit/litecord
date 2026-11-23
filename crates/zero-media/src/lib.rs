#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("Audio device error: {0}")]
    Device(String),
    #[error("Audio stream error: {0}")]
    Stream(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AudioDeviceList {
    pub inputs: Vec<AudioDeviceInfo>,
    pub outputs: Vec<AudioDeviceInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSettings {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub input_volume: f32,
    pub output_volume: f32,
    pub mic_muted: bool,
    pub deafened: bool,
    pub vad_threshold: f32,
    pub push_to_talk: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            input_volume: 1.0,
            output_volume: 0.8,
            mic_muted: false,
            deafened: false,
            vad_threshold: 0.02,
            push_to_talk: false,
        }
    }
}

pub fn enumerate_audio_devices() -> AudioDeviceList {
    let host = cpal::default_host();
    let default_in = host.default_input_device().and_then(|d| d.name().ok());
    let default_out = host.default_output_device().and_then(|d| d.name().ok());

    let inputs = host
        .input_devices()
        .map(|devs| {
            devs.filter_map(|d| d.name().ok())
                .map(|name| AudioDeviceInfo {
                    is_default: default_in.as_deref() == Some(name.as_str()),
                    name,
                })
                .collect()
        })
        .unwrap_or_default();

    let outputs = host
        .output_devices()
        .map(|devs| {
            devs.filter_map(|d| d.name().ok())
                .map(|name| AudioDeviceInfo {
                    is_default: default_out.as_deref() == Some(name.as_str()),
                    name,
                })
                .collect()
        })
        .unwrap_or_default();

    AudioDeviceList { inputs, outputs }
}

pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

pub struct MicMonitor {
    level_bits: Arc<AtomicU32>,
    running: Arc<AtomicBool>,
    _stream: cpal::Stream,
}

impl MicMonitor {
    pub fn start(device_name: Option<&str>) -> Result<Self, MediaError> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(target) => host
                .input_devices()
                .map_err(|e| MediaError::Device(e.to_string()))?
                .find(|d| d.name().ok().as_deref() == Some(target))
                .or_else(|| host.default_input_device()),
            None => host.default_input_device(),
        }
        .ok_or_else(|| MediaError::Device("No audio input device available".into()))?;

        let config = device
            .default_input_config()
            .map_err(|e| MediaError::Device(e.to_string()))?;

        let level_bits = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let level_ref = Arc::clone(&level_bits);
        let running = Arc::new(AtomicBool::new(true));

        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    let rms = compute_rms(data);
                    level_ref.store(rms.to_bits(), Ordering::Relaxed);
                },
                move |err| {
                    tracing::warn!("Audio input stream error: {err}");
                },
                None,
            )
            .map_err(|e| MediaError::Stream(e.to_string()))?;

        stream
            .play()
            .map_err(|e| MediaError::Stream(e.to_string()))?;

        Ok(Self {
            level_bits,
            running,
            _stream: stream,
        })
    }

    pub fn current_level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed))
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for MicMonitor {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// Plays a brief 440 Hz test chime on the selected (or default) output device.
pub fn play_test_chime(device_name: Option<String>, volume: f32) -> Result<(), MediaError> {
    let host = cpal::default_host();
    let device = match device_name.as_deref() {
        Some(target) => host
            .output_devices()
            .map_err(|e| MediaError::Device(e.to_string()))?
            .find(|d| d.name().ok().as_deref() == Some(target))
            .or_else(|| host.default_output_device()),
        None => host.default_output_device(),
    }
    .ok_or_else(|| MediaError::Device("No audio output device available".into()))?;

    let config = device
        .default_output_config()
        .map_err(|e| MediaError::Device(e.to_string()))?;
    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;
    let gain = volume.clamp(0.0, 1.0) * 0.18;

    let mut phase = 0.0f32;
    let step = 440.0 * std::f32::consts::TAU / sample_rate;

    let stream = device
        .build_output_stream(
            &config.into(),
            move |output: &mut [f32], _| {
                for frame in output.chunks_mut(channels.max(1)) {
                    let sample = phase.sin() * gain;
                    phase = (phase + step) % std::f32::consts::TAU;
                    for s in frame {
                        *s = sample;
                    }
                }
            },
            |err| tracing::warn!("Audio output stream error: {err}"),
            None,
        )
        .map_err(|e| MediaError::Stream(e.to_string()))?;

    stream
        .play()
        .map_err(|e| MediaError::Stream(e.to_string()))?;
    std::thread::sleep(Duration::from_millis(280));
    drop(stream);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCaptureFrame {
    pub display_id: u32,
    pub width: u32,
    pub height: u32,
    pub byte_len: usize,
    pub capture_ms: u64,
}

pub fn enumerate_displays() -> Vec<DisplayInfo> {
    screenshots::Screen::all()
        .map(|screens| {
            screens
                .into_iter()
                .map(|s| DisplayInfo {
                    id: s.display_info.id,
                    width: s.display_info.width,
                    height: s.display_info.height,
                    is_primary: s.display_info.is_primary,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn capture_primary_screen() -> Result<ScreenCaptureFrame, MediaError> {
    let screens =
        screenshots::Screen::all().map_err(|e| MediaError::Device(format!("Display enum: {e}")))?;
    let screen = screens
        .iter()
        .find(|s| s.display_info.is_primary)
        .or_else(|| screens.first())
        .ok_or_else(|| MediaError::Device("No display found for screen capture".into()))?;

    let started = std::time::Instant::now();
    let img = screen
        .capture()
        .map_err(|e| MediaError::Stream(format!("Screen capture failed: {e}")))?;
    let capture_ms = started.elapsed().as_millis() as u64;
    let width = img.width();
    let height = img.height();
    let byte_len = img.as_raw().len();

    Ok(ScreenCaptureFrame {
        display_id: screen.display_info.id,
        width,
        height,
        byte_len,
        capture_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_devices_does_not_panic() {
        let list = enumerate_audio_devices();
        // On headless CI there may be 0 devices; on desktop there are >= 1.
        let _ = (list.inputs.len(), list.outputs.len());
    }

    #[test]
    fn enumerate_displays_does_not_panic() {
        let displays = enumerate_displays();
        let _ = displays.len();
    }

    #[test]
    fn compute_rms_matches_expected_amplitude() {
        assert_eq!(compute_rms(&[]), 0.0);
        assert_eq!(compute_rms(&[0.0, 0.0, 0.0]), 0.0);
        let rms = compute_rms(&[0.5, -0.5, 0.5, -0.5]);
        assert!((rms - 0.5).abs() < 0.001);
    }
}

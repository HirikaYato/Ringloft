//! Вывод через cpal и сам RT-колбэк.
//!
//! На Linux `cpal::default_host()` сам предпочитает PipeWire (фича `pipewire`),
//! на Windows это WASAPI. Внутри колбэка: чтение кольца, плавная громкость,
//! ограничитель — и всё. Ни аллокаций, ни блокировок, ни логов.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SupportedStreamConfig};
use serde::Serialize;
use ts_rs::TS;

use super::ring::{RingConsumer, RingShared};
use super::tap::TapProducer;
use crate::error::{RingloftError, Result};

/// Коэффициент сглаживания громкости на фрейм: ~10 мс до цели при 48 кГц.
/// Нужен, чтобы резкий сдвиг ползунка не давал щелчка.
const GAIN_SMOOTHING: f32 = 0.002;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "AudioDevice.ts")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u32,
}

pub fn list_output_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    let default_id = host
        .default_output_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.to_string());

    let devices = match host.output_devices() {
        Ok(devices) => devices,
        Err(err) => {
            tracing::error!(%err, "не удалось перечислить устройства вывода");
            return Vec::new();
        }
    };

    let listed = devices.filter_map(|device| {
        let id = device.id().ok()?.to_string();
        let config = device.default_output_config().ok()?;
        Some(AudioDevice {
            is_default: Some(&id) == default_id.as_ref(),
            id,
            name: device.to_string(),
            sample_rate: config.sample_rate(),
            channels: u32::from(config.channels()),
        })
    });
    usable(listed)
}

/// Отсеивает то, что устройством вывода на самом деле не является.
///
/// Через PipeWire cpal показывает ещё и потоки чужих приложений: у них
/// пустое имя и одинаковый идентификатор на все потоки одной программы
/// (три строки `pipewire:Zen` от браузера — обычное дело). Выбирать такое
/// бессмысленно, а повторяющийся идентификатор вдобавок ломает список
/// во фронте.
fn usable(devices: impl Iterator<Item = AudioDevice>) -> Vec<AudioDevice> {
    let mut seen = std::collections::HashSet::new();
    devices
        .filter(|device| !device.name.is_empty() && device.name != "unknown")
        .filter(|device| seen.insert(device.id.clone()))
        .collect()
}

pub struct OutputStream {
    stream: cpal::Stream,
    pub device_name: String,
}

impl OutputStream {
    pub fn start(&self) -> Result<()> {
        self.stream
            .play()
            .map_err(|err| RingloftError::AudioDevice(err.to_string()))
    }
}

/// Выбранное устройство и конфиг. Отдельный шаг нужен потому, что ёмкость
/// кольца считается по частоте и числу каналов, а кольцо должно существовать
/// раньше потока: консьюмер уезжает внутрь колбэка.
pub struct OutputTarget {
    device: Device,
    config: SupportedStreamConfig,
}

impl OutputTarget {
    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate()
    }

    pub fn channels(&self) -> usize {
        usize::from(self.config.channels())
    }

}

/// `preferred_rate` — частота файла: если устройство её поддерживает, играем
/// без ресемплинга.
pub fn select(device_id: Option<&str>, preferred_rate: Option<u32>) -> Result<OutputTarget> {
    let host = cpal::default_host();
    let device = find_device(&host, device_id)?;
    let config = choose_config(&device, preferred_rate)?;
    Ok(OutputTarget { device, config })
}

pub fn open(
    target: OutputTarget,
    mut consumer: RingConsumer,
    mut tap: TapProducer,
    error_tx: crossbeam_channel::Sender<String>,
) -> Result<OutputStream> {
    let OutputTarget { device, config: supported } = target;
    let device_name = device.to_string();
    let sample_rate = supported.sample_rate();
    let channels = usize::from(supported.channels());
    let config = supported.config();

    let shared: Arc<RingShared> = Arc::clone(consumer.shared());
    let mut gain = shared.volume();

    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                consumer.sync_generation();

                if !shared.playing() {
                    out.fill(0.0);
                    return;
                }

                let taken = consumer.read(out);
                if taken < out.len() {
                    out[taken..].fill(0.0);
                    if shared.expecting_data() {
                        consumer.note_underrun();
                    }
                }

                let target = shared.volume() * shared.fade();
                for frame in out[..taken].chunks_mut(channels) {
                    gain += (target - gain) * GAIN_SMOOTHING;
                    for sample in frame.iter_mut() {
                        *sample = (*sample * gain).clamp(-1.0, 1.0);
                    }
                }

                // Отвод для анализатора спектра: он видит ровно то, что ушло
                // в устройство, вместе с громкостью и эквалайзером.
                tap.push(&out[..taken]);

                consumer.advance_frames(taken / channels);
            },
            move |err| {
                // В колбэке ошибок можно чуть больше, но всё равно без блокировок.
                let _ = error_tx.try_send(err.to_string());
            },
            None,
        )
        .map_err(|err| RingloftError::AudioDevice(err.to_string()))?;

    tracing::info!(device = %device_name, sample_rate, channels, "открыт аудиовыход");

    Ok(OutputStream {
        stream,
        device_name,
    })
}

fn find_device(host: &cpal::Host, device_id: Option<&str>) -> Result<Device> {
    if let Some(wanted) = device_id {
        let found = host.output_devices().ok().and_then(|mut devices| {
            devices.find(|device| device.id().is_ok_and(|id| id.to_string() == wanted))
        });
        if let Some(device) = found {
            return Ok(device);
        }
        tracing::warn!(%wanted, "устройство из настроек не найдено, беру системное");
    }

    host.default_output_device()
        .ok_or_else(|| RingloftError::AudioDevice("в системе нет устройства вывода".into()))
}

/// Частоты, на которых устройство откроется без пересчёта. Через PipeWire
/// это `clock.allowed-rates` из его настроек: по умолчанию там одна 48000.
pub fn supported_rates(device_id: Option<&str>) -> Vec<u32> {
    let host = cpal::default_host();
    let Ok(device) = find_device(&host, device_id) else {
        return Vec::new();
    };
    let Ok(ranges) = device.supported_output_configs() else {
        return Vec::new();
    };
    let mut rates: Vec<u32> = ranges
        .filter(|range| range.sample_format() == SampleFormat::F32)
        .flat_map(|range| {
            COMMON_RATES.iter().copied().filter(move |rate| {
                (range.min_sample_rate()..=range.max_sample_rate()).contains(rate)
            })
        })
        .collect();
    rates.sort_unstable();
    rates.dedup();
    rates
}

/// Частоты, которые встречаются в файлах; диапазоны устройств сводим к ним.
const COMMON_RATES: [u32; 9] = [
    22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 384_000,
];

/// Выбирает конфиг: только f32, предпочтительно стерео и частота файла.
fn choose_config(device: &Device, preferred_rate: Option<u32>) -> Result<SupportedStreamConfig> {
    let ranges: Vec<_> = device
        .supported_output_configs()
        .map_err(|err| RingloftError::AudioDevice(err.to_string()))?
        .filter(|range| range.sample_format() == SampleFormat::F32)
        .collect();

    if ranges.is_empty() {
        return Err(RingloftError::AudioDevice(
            "устройство не умеет f32 (другие форматы появятся позже)".into(),
        ));
    }

    let channels = ranges
        .iter()
        .map(|range| range.channels())
        .filter(|count| *count >= 2)
        .min()
        .or_else(|| ranges.iter().map(|range| range.channels()).max())
        .unwrap_or(2);

    let candidates: Vec<_> = ranges
        .into_iter()
        .filter(|range| range.channels() == channels)
        .collect();

    let device_default = device
        .default_output_config()
        .ok()
        .map(|config| config.sample_rate());

    for rate in [preferred_rate, device_default, Some(48_000), Some(44_100)]
        .into_iter()
        .flatten()
    {
        for range in &candidates {
            if let Some(config) = (*range).try_with_sample_rate(rate) {
                return Ok(config);
            }
        }
    }

    candidates
        .into_iter()
        .next()
        .map(|range| range.with_max_sample_rate())
        .ok_or_else(|| RingloftError::AudioDevice("нет подходящего конфига вывода".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, name: &str) -> AudioDevice {
        AudioDevice {
            id: id.to_owned(),
            name: name.to_owned(),
            is_default: false,
            sample_rate: 48_000,
            channels: 2,
        }
    }

    /// PipeWire показывает потоки чужих программ как устройства: имя пустое,
    /// а идентификатор повторяется. В списке им не место — и потому, что
    /// выбрать их нельзя, и потому, что дубль ломает список во фронте.
    #[test]
    fn drops_streams_and_duplicates() {
        let listed = [
            device("pipewire:sink_default", "default_sink"),
            device("pipewire:Zen", "unknown"),
            device("pipewire:alsa_output.usb-fifine", "fifine Ampli1"),
            device("pipewire:Zen", "unknown"),
            device("pipewire:alsa_output.usb-fifine", "fifine Ampli1"),
            device("pipewire:empty", ""),
        ];

        let kept = usable(listed.into_iter());
        let ids: Vec<&str> = kept.iter().map(|device| device.id.as_str()).collect();
        assert_eq!(ids, ["pipewire:sink_default", "pipewire:alsa_output.usb-fifine"]);
    }
}

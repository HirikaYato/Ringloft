//! Приведение числа каналов файла к числу каналов устройства.

/// Пишет в `out` interleaved-данные с `out_channels` каналами.
///
/// Моно раскладываем по всем каналам, стерео в моно — усредняем, в остальных
/// случаях копируем совпадающие каналы, лишние на устройстве глушим.
pub fn map_channels(input: &[f32], in_channels: usize, out_channels: usize, out: &mut Vec<f32>) {
    if in_channels == out_channels {
        out.extend_from_slice(input);
        return;
    }

    let frames = input.len() / in_channels.max(1);
    out.reserve(frames * out_channels);

    if in_channels == 1 {
        for &sample in input {
            for _ in 0..out_channels {
                out.push(sample);
            }
        }
        return;
    }

    if out_channels == 1 {
        for frame in input.chunks_exact(in_channels) {
            let sum: f32 = frame.iter().sum();
            out.push(sum / in_channels as f32);
        }
        return;
    }

    let common = in_channels.min(out_channels);
    for frame in input.chunks_exact(in_channels) {
        out.extend_from_slice(&frame[..common]);
        for _ in common..out_channels {
            out.push(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_goes_to_every_channel() {
        let mut out = Vec::new();
        map_channels(&[0.5, -0.5], 1, 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn stereo_downmixes_to_mono() {
        let mut out = Vec::new();
        map_channels(&[1.0, 0.0, 0.5, 0.5], 2, 1, &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn extra_device_channels_are_silent() {
        let mut out = Vec::new();
        map_channels(&[1.0, 2.0], 2, 4, &mut out);
        assert_eq!(out, vec![1.0, 2.0, 0.0, 0.0]);
    }
}

/// Conservative CPU-only audio preparation for local ASR.
///
/// The algorithm deliberately keeps 300 ms before detected speech, so VAD can
/// trim idle silence without cutting initial consonants. Noise suppression is
/// a light high-pass/DC blocker plus attenuation below the measured noise floor;
/// it does not require a GPU or a neural denoiser.
pub fn prepare_pcm_i16(
    bytes: &[u8],
    sample_rate: u32,
    vad_enabled: bool,
    noise_suppression_enabled: bool,
) -> Vec<u8> {
    let mut samples = bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32_768.0)
        .collect::<Vec<_>>();
    if samples.is_empty() {
        return Vec::new();
    }

    if noise_suppression_enabled {
        let mut previous_input = 0.0f32;
        let mut previous_output = 0.0f32;
        for sample in &mut samples {
            let input = *sample;
            let output = input - previous_input + 0.985 * previous_output;
            previous_input = input;
            previous_output = output;
            *sample = output.clamp(-1.0, 1.0);
        }
    }

    let frame_len = (sample_rate as usize / 50).max(1); // 20 ms
    let rms_values = samples
        .chunks(frame_len)
        .map(|frame| {
            (frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32).sqrt()
        })
        .collect::<Vec<_>>();
    let mut sorted_rms = rms_values.clone();
    sorted_rms.sort_by(|left, right| left.total_cmp(right));
    let noise_floor = sorted_rms
        .get(sorted_rms.len().saturating_sub(1) / 5)
        .copied()
        .unwrap_or(0.0);
    let peak_rms = rms_values.iter().copied().fold(0.0f32, f32::max);
    let base_speech_threshold = (noise_floor * 2.8).clamp(0.004, 0.035);
    // In short push-to-talk dictation there may be no leading silence. The
    // 20th percentile is then quiet speech rather than a noise floor, and the
    // old 2.8x rule could place the threshold above every frame. Natural speech
    // is amplitude-modulated, so cap the threshold relative to its peak when
    // there is enough dynamic contrast. Flat low-level room noise remains below
    // the original threshold and is still rejected.
    let has_voice_modulation = peak_rms >= 0.004 && peak_rms >= noise_floor * 1.08;
    let speech_threshold = if has_voice_modulation {
        base_speech_threshold.min((peak_rms * 0.78).max(0.003))
    } else {
        base_speech_threshold
    };

    if noise_suppression_enabled {
        let gate = (noise_floor * 1.4).max(0.0025);
        for (frame_index, frame) in samples.chunks_mut(frame_len).enumerate() {
            if rms_values.get(frame_index).copied().unwrap_or_default() < gate {
                for sample in frame {
                    *sample *= 0.35;
                }
            }
        }
    }

    let (start, end) = if vad_enabled {
        let first_speech = rms_values.iter().position(|rms| *rms >= speech_threshold);
        let last_speech = rms_values.iter().rposition(|rms| *rms >= speech_threshold);
        match (first_speech, last_speech) {
            (Some(first), Some(last)) => {
                let pre_roll_frames = 15; // 300 ms
                let tail_frames = 10; // 200 ms
                (
                    first.saturating_sub(pre_roll_frames) * frame_len,
                    ((last + tail_frames + 1) * frame_len).min(samples.len()),
                )
            }
            _ => return Vec::new(),
        }
    } else {
        (0, samples.len())
    };

    samples[start..end]
        .iter()
        .flat_map(|sample| ((*sample * 32_767.0).clamp(-32_768.0, 32_767.0) as i16).to_le_bytes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vad_keeps_preroll_before_speech() {
        let mut samples = vec![0i16; 16_000];
        for sample in &mut samples[8_000..12_000] {
            *sample = 8_000;
        }
        let bytes = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>();
        let output = prepare_pcm_i16(&bytes, 16_000, true, false);
        assert!(output.len() < bytes.len());
        assert!(output.len() >= 10_000); // speech plus conservative pre/tail roll
    }

    #[test]
    fn vad_rejects_silence_instead_of_inviting_hallucinations() {
        let bytes = vec![0u8; 16_000 * 2];
        assert!(prepare_pcm_i16(&bytes, 16_000, true, true).is_empty());
    }

    #[test]
    fn vad_keeps_quiet_short_speech_without_leading_silence() {
        // A 600 ms push-to-talk utterance whose entire capture contains quiet,
        // naturally modulated speech. The former noise-floor estimate rejected
        // all of it because its threshold was higher than the loudest frame.
        let frame_len = 16_000 / 50;
        let samples = (0..30)
            .flat_map(|frame| {
                let amplitude = if frame % 3 == 0 { 900i16 } else { 700i16 };
                std::iter::repeat_n(amplitude, frame_len)
            })
            .collect::<Vec<_>>();
        let bytes = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>();

        let output = prepare_pcm_i16(&bytes, 16_000, true, false);

        assert!(!output.is_empty());
    }

    #[test]
    fn vad_still_rejects_flat_low_level_background_noise() {
        let samples = vec![180i16; 16_000];
        let bytes = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>();

        assert!(prepare_pcm_i16(&bytes, 16_000, true, false).is_empty());
    }
}

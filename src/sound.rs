use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundEffect {
    Copy,
    Paste,
}

pub fn play(effect: SoundEffect, volume: u8) {
    let volume = (volume as f32 / 100.0).clamp(0.0, 1.0);
    if volume <= 0.0 {
        return;
    }
    thread::spawn(move || {
        let wav = synth_effect(effect, volume);
        if !play_with_stdin("aplay", &["-q", "-"], &wav) {
            let _ = play_with_stdin("paplay", &["/dev/stdin"], &wav);
        }
    });
}

fn play_with_stdin(program: &str, args: &[&str], wav: &[u8]) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if child
        .stdin
        .as_mut()
        .is_some_and(|stdin| stdin.write_all(wav).is_err())
    {
        return false;
    }
    child.wait().is_ok_and(|status| status.success())
}

fn synth_effect(effect: SoundEffect, volume: f32) -> Vec<u8> {
    const SAMPLE_RATE: u32 = 44_100;
    let mut samples = Vec::new();
    match effect {
        SoundEffect::Copy => append_tone(&mut samples, 500.0, 60, volume * 0.8, SAMPLE_RATE),
        SoundEffect::Paste => {
            append_tone(&mut samples, 950.0, 90, volume * 0.9, SAMPLE_RATE);
            append_silence(&mut samples, 110, SAMPLE_RATE);
            append_tone(&mut samples, 1150.0, 75, volume * 0.75, SAMPLE_RATE);
        }
    }
    wav_from_samples(&samples, SAMPLE_RATE)
}

fn append_tone(
    samples: &mut Vec<i16>,
    freq_hz: f32,
    duration_ms: u32,
    volume: f32,
    sample_rate: u32,
) {
    let len = sample_rate as usize * duration_ms as usize / 1000;
    let attack = (sample_rate as usize * 4 / 1000).max(1);
    let release = (sample_rate as usize * 22 / 1000).max(1).min(len.max(1));
    for i in 0..len {
        let t = i as f32 / sample_rate as f32;
        let phase = (t * freq_hz).fract();
        let triangle = 4.0 * (phase - 0.5).abs() - 1.0;
        let envelope = if i < attack {
            i as f32 / attack as f32
        } else if i + release >= len {
            (len.saturating_sub(i) as f32 / release as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };
        samples
            .push((triangle * envelope * volume.clamp(0.0, 1.0) * i16::MAX as f32 * 0.18) as i16);
    }
}

fn append_silence(samples: &mut Vec<i16>, duration_ms: u32, sample_rate: u32) {
    let len = sample_rate as usize * duration_ms as usize / 1000;
    samples.extend(std::iter::repeat_n(0, len));
}

fn wav_from_samples(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

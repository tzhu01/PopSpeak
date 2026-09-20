//! Explicit local benchmark, not compiled into the shipped application.
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, time::Instant};
use transcribe_cpp::{Backend, Model, ModelOptions, RunOptions, SessionOptions, TimestampKind};

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let path = Path::new(args.get(1).context("model.gguf required")?);
    let wav = args.get(2).context("16kHz mono WAV required")?;
    let threads: i32 = args.get(3).map(String::as_str).unwrap_or("4").parse()?;
    let mut reader = hound::WavReader::open(wav)?;
    anyhow::ensure!(
        reader.spec().sample_rate == 16000
            && reader.spec().channels == 1
            && reader.spec().bits_per_sample == 16,
        "need 16k mono PCM16"
    );
    let pcm = reader
        .samples::<i16>()
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .map(|v| v as f32 / 32768.0)
        .collect::<Vec<_>>();

    let start = Instant::now();
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    let sha = format!("{:x}", hash.finalize());
    println!("BENCH hash_ms={} sha256={sha}", start.elapsed().as_millis());
    anyhow::ensure!(
        sha == "034c557fe92ff8fcd9a9c041cbdaad347be0a86a58d3a348f63cf3f0180879d0",
        "unexpected Qwen fixture hash"
    );
    let start = Instant::now();
    let model = Model::load_with(
        path,
        &ModelOptions {
            backend: Backend::Cpu,
            gpu_device: 0,
        },
    )?;
    println!(
        "BENCH load_ms={} threads={threads} audio_s={:.3}",
        start.elapsed().as_millis(),
        pcm.len() as f64 / 16000.0
    );
    for run in 1..=2 {
        // Match production: retained weights, fresh session for every recording.
        let start = Instant::now();
        let mut session = model.session_with(&SessionOptions {
            n_threads: threads,
            ..SessionOptions::default()
        })?;
        let session_ms = start.elapsed().as_millis();
        let start = Instant::now();
        let result = session.run(
            &pcm,
            &RunOptions {
                timestamps: TimestampKind::None,
                ..RunOptions::default()
            },
        )?;
        println!(
            "BENCH run={run} session_ms={session_ms} native_run_ms={} text={:?} timings={:?}",
            start.elapsed().as_millis(),
            result.text,
            result.timings
        );
        anyhow::ensure!(!result.text.trim().is_empty(), "empty transcription");
    }
    Ok(())
}

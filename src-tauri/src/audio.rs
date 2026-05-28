use rodio::{OutputStream, Sink, Source};
use std::sync::mpsc::{channel, Sender};
use anyhow::Result;

#[derive(Debug)]
pub enum AudioCommand {
    Play(Vec<f32>, u32),
    Append(Vec<f32>, u32),
    Stop,
    Pause,
    Resume,
}

pub struct AudioPlayer {
    sender: Sender<AudioCommand>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = channel::<AudioCommand>();
        std::thread::spawn(move || {
            let Ok((_stream, stream_handle)) = OutputStream::try_default() else {
                eprintln!("[audio] Failed to create OutputStream");
                return;
            };
            println!("[audio] OutputStream created successfully");
            let mut current_sink: Option<Sink> = None;
            while let Ok(cmd) = receiver.recv() {
                match cmd {
                    AudioCommand::Play(samples, sample_rate) => {
                        println!("[audio] Play command received, {} samples @ {} Hz", samples.len(), sample_rate);
                        if let Some(ref sink) = current_sink {
                            sink.stop();
                        }
                        if let Ok(sink) = Sink::try_new(&stream_handle) {
                            let source = WavBufferSource::new(samples, sample_rate);
                            sink.append(source);
                            sink.play();
                            current_sink = Some(sink);
                            println!("[audio] Playback started");
                        } else {
                            eprintln!("[audio] Failed to create Sink");
                        }
                    }
                    AudioCommand::Append(samples, sample_rate) => {
                        if let Some(ref sink) = current_sink {
                            sink.append(WavBufferSource::new(samples, sample_rate));
                        } else if let Ok(sink) = Sink::try_new(&stream_handle) {
                            sink.append(WavBufferSource::new(samples, sample_rate));
                            sink.play();
                            current_sink = Some(sink);
                        }
                    }
                    AudioCommand::Stop => {
                        println!("[audio] Stop command received");
                        if let Some(ref sink) = current_sink {
                            sink.stop();
                            current_sink = None;
                        }
                    }
                    AudioCommand::Pause => {
                        if let Some(ref sink) = current_sink {
                            sink.pause();
                        }
                    }
                    AudioCommand::Resume => {
                        if let Some(ref sink) = current_sink {
                            sink.play();
                        }
                    }
                }
            }
            println!("[audio] Audio thread exiting");
        });
        Ok(AudioPlayer { sender })
    }

    pub fn play_wav(&self, samples: Vec<f32>, sample_rate: u32) -> Result<()> {
        self.sender.send(AudioCommand::Play(samples, sample_rate))?;
        Ok(())
    }

    pub fn append_wav(&self, samples: Vec<f32>, sample_rate: u32) -> Result<()> {
        self.sender.send(AudioCommand::Append(samples, sample_rate))?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.sender.send(AudioCommand::Stop)?;
        Ok(())
    }

    pub fn pause(&self) -> Result<()> {
        self.sender.send(AudioCommand::Pause)?;
        Ok(())
    }

    pub fn resume(&self) -> Result<()> {
        self.sender.send(AudioCommand::Resume)?;
        Ok(())
    }
}

struct WavBufferSource {
    samples: Vec<f32>,
    sample_rate: u32,
    pos: usize,
}

impl WavBufferSource {
    fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        WavBufferSource { samples, sample_rate, pos: 0 }
    }
}

impl Iterator for WavBufferSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.pos < self.samples.len() {
            let sample = self.samples[self.pos];
            self.pos += 1;
            Some(sample)
        } else {
            None
        }
    }
}

impl Source for WavBufferSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 1 }
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn total_duration(&self) -> Option<std::time::Duration> {
        let secs = self.samples.len() as f64 / self.sample_rate as f64;
        Some(std::time::Duration::from_secs_f64(secs))
    }
}

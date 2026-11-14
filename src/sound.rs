//! The tap sound. Every sound is read into memory once, and played on rodio's mixer from a
//! thread of its own, so a slow audio device never holds up the pointer.

use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};

use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, Source};

use crate::hands;
use crate::settings::TAP_SOUNDS;

enum Job {
    Load(String),
    Volume(f32),
    Play,
    Preview(String),
}

pub struct Sound {
    jobs: std::sync::Mutex<Option<Sender<Job>>>,
}

impl Default for Sound {
    fn default() -> Self {
        Self::new()
    }
}

impl Sound {
    /// Starts without a device; `start` opens it.
    pub fn new() -> Sound {
        Sound { jobs: std::sync::Mutex::new(None) }
    }

    pub fn start(&self, choice: &str, volume: f32) {
        let (tx, rx) = channel::<Job>();
        let choice = choice.to_string();
        let mut volume = volume;
        std::thread::Builder::new()
            .name("waypointer sound".into())
            .spawn(move || {
                let sounds = load_all();
                let mut current = choice;
                let mut sink: Option<rodio::MixerDeviceSink> = None;
                for job in rx {
                    let name = match job {
                        Job::Load(name) => {
                            current = name;
                            continue;
                        }
                        Job::Volume(v) => {
                            volume = v;
                            continue;
                        }
                        Job::Play => current.clone(),
                        Job::Preview(name) => name,
                    };
                    if sink.is_none() {
                        sink = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| eprintln!("waypointer: no audio device: {e}")).ok();
                    }
                    if volume <= 0.0 {
                        continue;
                    }
                    if let (Some(sink), Some(buffer)) = (&sink, sounds.get(&name)) {
                        sink.mixer().add(buffer.clone().amplify(volume));
                    }
                }
            })
            .ok();
        *self.jobs.lock().unwrap() = Some(tx);
    }

    fn send(&self, job: Job) {
        if let Some(tx) = self.jobs.lock().unwrap().as_ref() {
            let _ = tx.send(job);
        }
    }

    pub fn load(&self, name: &str) {
        self.send(Job::Load(name.to_string()));
    }

    pub fn set_volume(&self, volume: f32) {
        self.send(Job::Volume(volume));
    }

    pub fn play(&self) {
        self.send(Job::Play);
    }

    pub fn preview(&self, name: &str) {
        self.send(Job::Preview(name.to_string()));
    }
}

fn load_all() -> HashMap<String, SamplesBuffer> {
    let mut out = HashMap::new();
    for (name, _) in TAP_SOUNDS {
        let Some(path) = hands::data_file(&format!("sounds/tap_{name}.wav")) else { continue };
        let Ok(file) = std::fs::File::open(&path) else { continue };
        let Ok(decoder) = Decoder::try_from(file) else { continue };
        let channels = decoder.channels();
        let rate = decoder.sample_rate();
        let samples: Vec<_> = decoder.collect();
        out.insert(name.to_string(), SamplesBuffer::new(channels, rate, samples));
    }
    out
}

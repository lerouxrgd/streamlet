use std::error::Error;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use gstreamer::{self as gst, glib, prelude::*};
use gstreamer_player as gst_player;
use subprocess::{Exec, Redirection};

const SPINNER: [&'static str; 8] = ["|", "/", "-", r"\", "|", "/", "-", r"\"];

pub struct Player {
    #[allow(dead_code)]
    main_loop_handle: thread::JoinHandle<()>,
    player: gst_player::Player,
    playing: Arc<AtomicBool>,
    uri: String,
    ydl_fetcher: Option<thread::JoinHandle<()>>,
    stream_info: Arc<Mutex<Option<StreamInfo>>>,
    spinner: AtomicUsize,
}

impl Player {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        gst::init()?;

        let main_loop_handle = thread::spawn(move || glib::MainLoop::new(None, false).run());

        let dispatcher = gst_player::PlayerGMainContextSignalDispatcher::new(None);
        let player = gst_player::Player::new(
            None,
            Some(&dispatcher.upcast::<gst_player::PlayerSignalDispatcher>()),
        );

        let playing = Arc::new(AtomicBool::new(false));
        let playing_c = playing.clone();
        player.connect_error(move |player, _err| {
            player.stop();
            playing_c.store(false, Ordering::SeqCst);
        });

        Ok(Self {
            main_loop_handle,
            player,
            playing,
            uri: String::default(),
            ydl_fetcher: None,
            stream_info: Arc::new(Mutex::new(None)),
            spinner: AtomicUsize::new(0),
        })
    }

    pub fn set_uri(&mut self, uri: &str) {
        if self.uri == uri {
            return;
        }
        self.uri = uri.to_string();

        let uri = uri.to_string();
        let player = self.player.clone();
        let playing = self.playing.clone();
        let stream_info = self.stream_info.clone();

        let fetcher_handle = thread::spawn(move || {
            let output = Exec::cmd("youtube-dl")
                .arg("-j")
                .arg(&uri)
                .stdout(Redirection::Pipe)
                .capture()
                .unwrap()
                .stdout_str();

            let video = serde_json::from_str::<serde_json::Value>(&output).unwrap();

            let formats = video["formats"].as_array().unwrap();
            let url = formats[0]["url"].as_str().unwrap();
            let title = video["title"].as_str().unwrap().to_string();

            player.set_uri(&url);
            player.set_video_track_enabled(false);
            player.play();
            playing.store(true, Ordering::SeqCst);

            let stream_info = &mut *stream_info.lock().unwrap();
            *stream_info = Some(StreamInfo { uri, title });
        });

        self.ydl_fetcher = Some(fetcher_handle);
    }

    pub fn progress(&self) -> f64 {
        let duration = self.player.get_duration().seconds().unwrap_or_else(|| 0);
        let position = self.player.get_position().seconds().unwrap_or_else(|| 0);

        if duration == 0 {
            0.0
        } else {
            position as f64 / duration as f64
        }
    }

    pub fn title(&self) -> String {
        if let Some(info) = &*self.stream_info.lock().unwrap() {
            if &self.uri != &info.uri {
                let i = self
                    .spinner
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |i| Some((i + 1) % 8));
                SPINNER[i.unwrap()].to_string()
            } else {
                let dur = self.player.get_duration().seconds().unwrap_or_else(|| 0);
                let pos = self.player.get_position().seconds().unwrap_or_else(|| 0);

                let dur_hh = dur / 3600;
                let dur_mm = (dur / 60) % 60;
                let dur_ss = dur % 60;
                let dur_fmt = if dur_hh > 0 {
                    format!("{:02}:{:02}:{:02}", dur_hh, dur_mm, dur_ss)
                } else {
                    format!("{:02}:{:02}", dur_mm, dur_ss)
                };

                let pos_hh = pos / 3600;
                let pos_mm = (pos / 60) % 60;
                let pos_ss = pos % 60;
                let pos_fmt = if pos_hh > 0 {
                    format!("{:02}:{:02}:{:02}", pos_hh, pos_mm, pos_ss)
                } else {
                    format!("{:02}:{:02}", pos_mm, pos_ss)
                };

                format!("{} ({} / {})", info.title, pos_fmt, dur_fmt)
            }
        } else {
            if &self.uri == "" {
                String::default()
            } else {
                let i = self
                    .spinner
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |i| Some((i + 1) % 8));
                SPINNER[i.unwrap()].to_string()
            }
        }
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }
    pub fn play(&self) {
        self.player.play();
        self.playing.store(true, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        self.player.pause();
        self.playing.store(false, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.player.stop();
        self.playing.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct StreamInfo {
    uri: String,
    title: String,
}

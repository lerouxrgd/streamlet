use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use futures::executor::ThreadPool;
use futures::future::FutureExt;
use gstreamer::{self as gst, glib, prelude::*};
use gstreamer_player as gst_player;
use lazy_static::lazy_static;

const SPINNER: [&'static str; 8] = ["|", "/", "-", r"\", "|", "/", "-", r"\"];

lazy_static! {
    pub static ref TP: ThreadPool = ThreadPool::new().unwrap();
}

pub struct Player {
    #[allow(dead_code)]
    glib_loop: thread::JoinHandle<()>,
    player: gst_player::Player,
    playing: Arc<AtomicBool>,
    uri: String,
    fetching: Arc<AtomicBool>,
    stream_info: Arc<Mutex<Option<StreamInfo>>>,
    spinner: AtomicUsize,
}

impl Player {
    pub fn new() -> Result<Self> {
        gst::init()?;

        let glib_loop = thread::spawn(move || glib::MainLoop::new(None, false).run());

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
            glib_loop,
            player,
            playing,
            uri: String::default(),
            fetching: Arc::new(AtomicBool::new(false)),
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
        let fetching = self.fetching.clone();
        let player = self.player.clone();
        let playing = self.playing.clone();
        let stream_info = self.stream_info.clone();

        self.fetching.store(true, Ordering::SeqCst);

        TP.spawn_ok(
            async move {
                let output = Command::new("youtube-dl")
                    .stderr(Stdio::null())
                    .args(&["--socket-timeout", "5", "-j", &uri])
                    .output()?
                    .stdout;
                let output = String::from_utf8(output)?;

                let meta = serde_json::from_str::<YtDlMeta>(&output)?;
                let url = meta.formats[0].url.clone();
                let title = meta.title;

                player.set_uri(Some(&url));
                player.set_video_track_enabled(false);
                player.play();
                playing.store(true, Ordering::SeqCst);

                let stream_info = &mut *stream_info.lock().unwrap();
                *stream_info = Some(StreamInfo { uri, title });

                Ok(())
            }
            .map(move |_: Result<()>| {
                fetching.store(false, Ordering::SeqCst);
            }),
        );
    }

    pub fn progress(&self) -> f64 {
        let duration = self
            .player
            .duration()
            .map(|d| d.seconds())
            .unwrap_or_else(|| 0);
        let position = self
            .player
            .position()
            .map(|d| d.seconds())
            .unwrap_or_else(|| 0);

        if duration == 0 {
            0.0
        } else {
            position as f64 / duration as f64
        }
    }

    fn spin(&self) -> String {
        let i = self
            .spinner
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |i| Some((i + 1) % 8));
        SPINNER[i.unwrap()].to_string()
    }

    pub fn title(&mut self) -> String {
        match &*self.stream_info.lock().unwrap() {
            None => {
                if self.fetching.load(Ordering::SeqCst) {
                    return self.spin();
                }

                String::default()
            }

            Some(info) => {
                if self.fetching.load(Ordering::SeqCst) {
                    return self.spin();
                }

                let dur = self
                    .player
                    .duration()
                    .map(|d| d.seconds())
                    .unwrap_or_else(|| 0);
                let pos = self
                    .player
                    .position()
                    .map(|d| d.seconds())
                    .unwrap_or_else(|| 0);

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

#[derive(Debug, serde::Deserialize)]
struct YtDlMeta {
    title: String,
    formats: Vec<YtDlFormat>,
}

#[derive(Debug, serde::Deserialize)]
struct YtDlFormat {
    url: String,
}

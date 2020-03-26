import logging as log
import os
import curses
import signal
import sys
import time

import npyscreen as nps

from datetime import datetime
from multiprocessing import Pipe, Process
from subprocess import Popen
from threading import Thread

from youtube_dl import YoutubeDL
from .thread_utils import KillableThread, synchronized

FNULL = open(os.devnull, "w")


class App(nps.NPSAppManaged):
    def onStart(self):
        self.addForm("MAIN", MainForm, name="streamlet")

    def onCleanExit(self):
        self.getForm("MAIN").destroy()


class MainForm(nps.Form):
    OK_BUTTON_TEXT = "Quit"

    def create(self):
        self.w_video_url = self.add(
            VideoUrlInput,
            parent_form=self,
            name="Stream URL:",
            value="https://www.youtube.com/watch?v=ukzOgoLjHLk",
            # value="https://www.youtube.com/watch?v=XivIbYWE0go",
        )

        self.w_playing = self.add(PlayingBarBox, rely=4, max_height=3,)

        self.w_play = self.add(
            PlayButtonBox,
            max_height=3,
            max_width=14,
            contained_widget_arguments={"parent_form": self},
        )

    def destroy(self):
        self.w_play.destroy()
        self.w_playing.destroy()
        self.w_video_url.destroy()

    def afterEditing(self):
        self.parentApp.setNextForm(None)


class VideoUrlInput(nps.TitleText):
    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self.parent_form = kw.get("parent_form", None)

        def prepare_video(_input):
            ydl_opts = {
                "format": "bestaudio/best",
                "retries": 10,
                "logger": log,
            }
            with YoutubeDL(ydl_opts) as ydl:
                try:
                    info = ydl.extract_info(self.value, download=False)
                    duration = info["duration"]

                    self.parent_form.w_play.stop()

                    self.parent_form.w_playing.name = info["title"]
                    self.parent_form.w_playing.set_duration(duration)
                    self.parent_form.w_playing.set_current_time(0)
                    self.parent_form.w_playing.display()

                    self.entry_widget.editing = False
                    self.entry_widget.how_exited = nps.wgwidget.EXITED_DOWN
                    self.display()

                    self.parent_form.editw = 2
                    self.parent_form.w_play.edit()

                except Exception:
                    nps.notify("Cannot process <Stream URL>", title="Error")
                    time.sleep(1)
                    return

        self.entry_widget.add_handlers({curses.ascii.LF: prepare_video})


class PlayingBar(nps.SliderNoLabel):
    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self.editable = False


def fmt_duration(seconds):
    seconds = int(seconds)
    hours, seconds = divmod(seconds, 3600)
    minutes, seconds = divmod(seconds, 60)
    if hours > 0:
        return "{:02}:{:02}:{:02}".format(hours, minutes, seconds)
    elif minutes > 0:
        return "{:02}:{:02}".format(minutes, seconds)
    else:
        return "{:02}".format(seconds)


class PlayingBarBox(nps.BoxTitle):
    _contained_widget = PlayingBar

    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self.t_anim = None

    @synchronized
    def display(self):
        super().display()

    def anim_on(self):
        if self.t_anim is not None:
            return

        def anim(w):
            while True:
                time.sleep(1 - datetime.utcnow().microsecond / 1000000.0)
                w.entry_widget.h_increase(1)
                w.set_current_time(w.entry_widget.value)
                w.display()

        self.t_anim = KillableThread(target=anim, args=(self,), daemon=True)
        self.t_anim.start()

    def anim_off(self):
        if self.t_anim is not None:
            self.t_anim.terminate()
            self.t_anim = None

    def reset(self):
        self.entry_widget.value = 0
        self.set_current_time(0)

    def set_current_time(self, seconds):
        self.footer = "{} / {}".format(
            fmt_duration(seconds), fmt_duration(self.entry_widget.out_of),
        )

    def set_duration(self, duration):
        self.entry_widget.out_of = duration


class PlayButton(nps.ButtonPress):
    PLAY = "\u25B6"
    PAUSE = "\u23F8"

    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self.parent_form = kw.get("parent_form", None)
        self.name = PlayButton.PLAY
        self.p_ydl = None
        self.p_ffplay = None

        def check_ffplay(this):
            while True:
                if this.p_ffplay is not None:
                    if this.p_ffplay.poll() is not None:
                        this.stop()
                time.sleep(1)

        self.t_checker = Thread(target=check_ffplay, args=(self,), daemon=True)
        self.t_checker.start()

    def whenPressed(self):
        if self.parent_form.w_playing.name is None:
            nps.notify("Stream is not set", title="Warning")
            time.sleep(1)
            return

        # Play music
        if self.name == PlayButton.PLAY:
            # Music was not started
            if self.p_ffplay is None:
                rx, tx = Pipe(duplex=False)
                rdr = os.fdopen(rx.fileno(), "r")

                ffplay_cmd = ["ffplay", "-nodisp", "-autoexit", "-hide_banner", "-"]
                self.p_ffplay = Popen(ffplay_cmd, stdin=rdr, stdout=FNULL, stderr=FNULL)

                def play(tx, video_url):
                    os.dup2(tx.fileno(), 1)
                    ydl_opts = {
                        "format": "bestaudio/best",
                        "outtmpl": "-",
                        "retries": 10,
                        "logger": log,
                    }
                    with YoutubeDL(ydl_opts) as ydl:
                        ydl.extract_info(video_url)

                video_url = self.parent_form.w_video_url.value
                self.p_ydl = Process(target=play, args=(tx, video_url))
                self.p_ydl.start()

                self.parent_form.w_playing.anim_on()

            # Music was paused
            else:
                self.parent_form.w_playing.anim_on()
                self.p_ffplay.send_signal(signal.SIGCONT)

            self.name = PlayButton.PAUSE

        # Pause music
        else:
            self.p_ffplay.send_signal(signal.SIGTSTP)
            self.parent_form.w_playing.anim_off()
            self.name = PlayButton.PLAY

    @synchronized
    def stop(self):
        self.destroy()
        self.parent_form.w_playing.anim_off()
        self.parent_form.w_playing.reset()
        self.parent_form.w_playing.display()
        self.name = PlayButton.PLAY
        self.display()

    def destroy(self):
        if self.p_ydl is not None:
            self.p_ydl.kill()
            self.p_ydl = None
        if self.p_ffplay is not None:
            self.p_ffplay.kill()
            self.p_ffplay = None


class PlayButtonBox(nps.BoxTitle):
    _contained_widget = PlayButton

    def stop(self):
        self.entry_widget.stop()

    def destroy(self):
        self.entry_widget.destroy()


def main():
    app = App()

    def kill_app(sig, frame):
        app.onCleanExit()
        sys.exit(0)

    signal.signal(signal.SIGINT, kill_app)

    log.basicConfig(
        format="%(levelname)s %(asctime)s %(filename)s:%(lineno)d %(message)s",
        filename="debug.log",
        level=log.DEBUG,
    )
    log.critical("=" * 70)

    app.run()

import time
import logging as log
import os
import signal
import sys
from multiprocessing import Pipe, Process
from subprocess import Popen

import npyscreen as nps
import youtube_dl

from .killable_thread import KillableThread

FNULL = open(os.devnull, "w")


class App(nps.NPSAppManaged):
    def onStart(self):
        self.addForm("MAIN", MainForm, name="streamlet")

    # TODO: cleanup subprocesses etc correctly here
    # def onCleanExit(self):
    #     log.debug("Bye!")


class MainForm(nps.Form):
    OK_BUTTON_TEXT = "Quit"

    def create(self):
        self.w_video_url = self.add(
            VideoUrlBox,
            max_height=3,
            name="Video",
            contained_widget_arguments={
                "name": "URL:",
                "value": "https://www.youtube.com/watch?v=htML2EzF2uI",
            },
        )

        # TODO: should also display a timer
        self.w_playing = self.add(
            PlayingBarBox,
            name="Playing",
            max_height=3,
            contained_widget_arguments={"out_of": 100},
        )

        self.w_play = self.add(
            PlayButtonBox,
            max_height=3,
            max_width=14,
            contained_widget_arguments={"parent_form": self},
        )

    def afterEditing(self):
        self.parentApp.setNextForm(None)


class VideoUrlBox(nps.BoxTitle):
    _contained_widget = nps.TitleText


class PlayingBar(nps.SliderNoLabel):
    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self.editable = False
        self.t_anim = None

    def anim_on(self):
        def anim(w):
            while True:
                w.h_increase(1)
                w.display()
                time.sleep(1)

        self.t_anim = KillableThread(target=anim, args=(self,))
        self.t_anim.start()

    def anim_off(self):
        self.t_anim.terminate()


class PlayingBarBox(nps.BoxTitle):
    _contained_widget = PlayingBar

    def set_duration(self, duration):
        self.entry_widget.out_of = duration

    def anim_on(self):
        self.entry_widget.anim_on()

    def anim_off(self):
        self.entry_widget.anim_off()


class PlayButton(nps.ButtonPress):
    PLAY = "\u25B6"
    PAUSE = "\u23F8"

    def __init__(self, *args, **kw):
        super().__init__(*args, **kw)
        self.parent_form = kw.get("parent_form", None)
        self.name = PlayButton.PLAY
        self.p_ffplay = None  # TODO: better handling of subprocess handle

    def whenPressed(self):
        # Play music
        if self.name == PlayButton.PLAY:
            # Music was not started
            if self.p_ffplay is None:
                video_url = self.parent_form.w_video_url.value

                ydl_opts = {
                    "format": "bestaudio/best",
                    "retries": 10,
                    "logger": log,
                }
                with youtube_dl.YoutubeDL(ydl_opts) as ydl:
                    info = ydl.extract_info(video_url, download=False)
                    duration = info["duration"]
                    self.parent_form.w_playing.set_duration(duration)
                    self.parent_form.w_playing.set_value(0)
                    self.parent_form.w_playing.update()

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
                    with youtube_dl.YoutubeDL(ydl_opts) as ydl:
                        ydl.extract_info(video_url)
                        # TODO: use some postprocess hook to close tx?

                p_ydl = Process(target=play, args=(tx, video_url))
                p_ydl.start()

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


class PlayButtonBox(nps.BoxTitle):
    _contained_widget = PlayButton


def main():
    # TODO: use App.onCleanExit() here
    signal.signal(signal.SIGINT, lambda sig, frame: sys.exit(0))

    log.basicConfig(
        format="%(levelname)s %(asctime)s %(filename)s:%(lineno)d %(message)s",
        filename="debug.log",
        level=log.DEBUG,
    )
    log.critical("=" * 70)

    App().run()

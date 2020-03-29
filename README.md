# streamlet - TUI for streaming music

**Requirements:** [ffmpeg] must be installed (in order to use `fflpay`). For now, only `Linux` is supported.

## Usage

TBD

## Installation

TBD

## Development

This project uses [poetry][], which should be easily installable with your package manager.
On Arch Linux for instance it's available through the package `python-poetry`.

To setup an appropriate virtalenv run `poetry install`. When dependencies change, update it
with `poetry update`.

Run `poetry shell` to start a virtualenv and use your favorite editor from there.
You will also have dev dependencies avaialble, that means [black][] for code formatting,
[flake8][] for linting, [jedi][] for autocompletion/static analysis/refactoring (provided
through [pyls][]).

To test as you develop, you can use `poetry run streamlet` and play with it.

[ffmpeg]: https://ffmpeg.org
[poetry]: https://python-poetry.org
[black]: https://github.com/psf/black
[flake8]: https://github.com/PyCQA/flake8
[jedi]: https://github.com/davidhalter/jedi
[pyls]: https://github.com/palantir/python-language-server

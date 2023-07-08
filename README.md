# Mpris ctl
A cli tool for managing media players using DBus Mpris2.
This repository consists of two programs,
the daemon and the cli.

The daemon looks for a player that is currently active
and the cli will use that as default target.

The cli may play, pause, toggle the target player.
If the daemon is not running, first player that is found will be used.
It may also skip songs and show metadata about the media currently playing.

## Usage
``` sh
> mpris-ctl
Manage dbus mpris2 players

Usage: mpris-ctl [OPTIONS] <COMMAND>

Commands:
  play      Send play media command
  pause     Send pause media command
  toggle    Send play if paused, else send pause
  prev      Switch to previous media/song
  next      Switch to next media/song
  metadata  Obtain metadata of the currently playing media
  status    Obtain status of the currently active player
  help      Print this message or the help of the given subcommand(s)

Options:
      --all-players
      --active-players
      --player <PLAYER>
  -h, --help             Print help
  -V, --version          Print version
```

`

## Motivation
I am making this tool mainly for learning Rust. If you want a program that does all
this one does, plus more, use [playerctl](https://github.com/altdesktop/playerctl).

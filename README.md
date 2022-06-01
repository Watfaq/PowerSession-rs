# PowerSession

> Record a Session in PowerShell.

PowerShell version of [asciinema](https://github.com/asciinema/asciinema) based on [Windows Pseudo Console(ConPTY)](https://devblogs.microsoft.com/commandline/windows-command-line-introducing-the-windows-pseudo-console-conpty/)

Basic features record/play/auth/upload are working now.

*This is a new Rust implemented version.*
*if you are looking for the C# implementation, please go to [the old version](https://github.com/Watfaq/PowerSession/tree/csharp)*

## Checkout A Demo

[![asciicast](https://asciinema.org/a/272866.svg)](https://asciinema.org/a/272866)

## Compatibilities

* The output is comptible with asciinema v2 standard and can be played by `ascinnema`.
* The `auth` and `upload` functionalities are against `asciinema.org`.

## Installation

```console
cargo install PowerSession
```

## Usage

### Get Help
```console
PS D:\projects\PowerSession> PowerSession.exe -h
PowerSession

USAGE:
    PowerSession.exe [SUBCOMMAND]

OPTIONS:
    -h, --help    Print help information

SUBCOMMANDS:
    rec       Record and save a session
    play
    auth      Authentication with asciinema.org
    upload    Upload a session to ascinema.org
    help      Print this message or the help of the given subcommand(s)
```

## Credits
- [windows-rs](https://github.com/microsoft/windows-rs)

## Supporters
- [GitBook](https://www.gitbook.com/) Community License

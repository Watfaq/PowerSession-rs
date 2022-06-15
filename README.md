# PowerSession

> **Record a Session in PowerShell.**

[![Crates.io](https://img.shields.io/crates/v/PowerSession?style=flat-square)](https://crates.io/crates/PowerSession)
[![Crates.io](https://img.shields.io/crates/d/PowerSession?style=flat-square)](https://crates.io/crates/PowerSession)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](./LICENSE)
[![Build Status](https://img.shields.io/github/workflow/status/Watfaq/PowerSession-rs/Rust%20CI/main?style=flat-square)](https://github.com/Watfaq/PowerSession-rs/actions/workflows/ci.yml?query=branch%3Amain)
[![Contributors](https://img.shields.io/github/contributors/Watfaq/PowerSession-rs?style=flat-square)](https://github.com/Watfaq/PowerSession-rs/graphs/contributors)

PowerShell version of [asciinema](https://github.com/asciinema/asciinema) based on [Windows Pseudo Console(ConPTY)](https://devblogs.microsoft.com/commandline/windows-command-line-introducing-the-windows-pseudo-console-conpty/)

*This is a new Rust implemented version.*
*if you are looking for the C# implementation, please go to the [C# version](https://github.com/Watfaq/PowerSession)*

## Checkout A Demo

[![asciicast](https://asciinema.org/a/499120.svg)](https://asciinema.org/a/499120)

## Installation

### Cargo

```console
cargo install PowerSession
```

### Scoop

```console
scoop install powersession-rs
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

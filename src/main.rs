#[allow(non_snake_case)]
extern crate clap;
mod commands;
mod terminal;

use clap::{AppSettings, Arg, Command};
use commands::{Asciinema, Auth, Play};
use commands::{Record, Upload};

fn main() {
    let app = Command::new("PowerSession")
        .setting(AppSettings::DeriveDisplayOrder)
        .subcommand(
            Command::new("rec")
                .about("Record and save a session")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .arg(
                    Arg::new("file")
                        .help("The filename to save the record")
                        .index(1)
                        .required(true),
                )
                .arg(
                    Arg::new("command")
                        .help("The command to record, default to be powershell.exe")
                        .takes_value(true)
                        .short('c')
                        .long("command")
                        .default_value("powershell.exe"),
                )
                .arg(
                    Arg::new("force")
                        .help("Overwrite if session already exists")
                        .takes_value(false)
                        .short('f')
                        .long("force"),
                ),
        )
        .subcommand(
            Command::new("play")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .override_help("Play a recorded session")
                .arg(Arg::new("file").help("The record session").index(1)),
        )
        .subcommand(
            Command::new("auth")
                .about("Authentication with asciinema.org")
                .subcommand_required(true)
                .arg_required_else_help(true),
        )
        .subcommand(
            Command::new("upload")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .about("Upload a session to ascinema.org")
                .arg(Arg::new("file").help("The file to be uploaded").index(1)),
        );
    let m = app.get_matches();

    match m.subcommand() {
        Some(("play", play_matches)) => {
            let play = Play::new(play_matches.value_of("file").unwrap().to_owned());
            play.execute();
        }
        Some(("rec", rec_matches)) => {
            let mut record = Record::new(
                rec_matches.value_of("file").unwrap().to_owned(),
                None,
                rec_matches.value_of("command").unwrap().to_owned(),
                rec_matches.is_present("force"),
            );
            record.execute();
        }
        Some(("auth", _)) => {
            let api_service = Asciinema::new();
            let auth = Auth::new(Box::new(api_service));
            auth.execute();
        }
        Some(("upload", upload_matches)) => {
            let api_service = Asciinema::new();
            let upload = Upload::new(
                Box::new(api_service),
                upload_matches.value_of("file").unwrap().to_owned(),
            );
            upload.execute();
        }
        _ => unreachable!(),
    }
}

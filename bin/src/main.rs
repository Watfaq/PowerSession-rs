#[allow(non_snake_case)]
extern crate clap;
extern crate commands;

use clap::{App, AppSettings, Arg};
use commands::{Asciinema, Auth, Play};
use commands::{Record, Upload};

fn main() {
    let app = App::new("PowerSession")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::ColoredHelp)
        .subcommand(
            App::new("rec")
                .about("Record and save a session")
                .arg(
                    Arg::new("file")
                        .about("The filename to save the record")
                        .index(1)
                        .required(true),
                )
                .arg(
                    Arg::new("command")
                        .about("The command to record, default to be powershell.exe")
                        .takes_value(true)
                        .short('c')
                        .long("command")
                        .default_value("powershell.exe"),
                )
                .arg(
                    Arg::new("force")
                        .about("Overwrite if session already exists")
                        .takes_value(false)
                        .short('f')
                        .long("force"),
                ),
        )
        .subcommand(
            App::new("play")
                .about("Play a recorded session")
                .arg(Arg::new("file").about("The record session").index(1)),
        )
        .subcommand(App::new("auth").about("Authentication with asciinema.org"))
        .subcommand(
            App::new("upload")
                .about("Upload a session to ascinema.org")
                .arg(Arg::new("file").about("The file to be uploaded").index(1)),
        );
    let m = app.get_matches();

    match m.subcommand() {
        Some(("play", play_matches)) => {
            let mut play = Play::new(play_matches.value_of("file").unwrap().to_owned());
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

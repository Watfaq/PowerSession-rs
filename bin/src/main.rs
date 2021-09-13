#[allow(non_snake_case)]
extern crate clap;
extern crate commands;

use clap::{App, AppSettings, Arg};
use commands::Record;

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
            println!("Playing {}", play_matches.value_of("file").unwrap())
        }
        Some(("rec", rec_matches)) => {
            let mut record = Record::new(
                rec_matches.value_of("file").unwrap().to_owned(),
                None,
                rec_matches.value_of("command").unwrap().to_owned(),
            );
            record.execute();
        }
        Some(("auth", _)) => {
            commands::test();
        }
        Some(("upload", upload_matches)) => {
            println!(
                "Uploading file {}",
                upload_matches.value_of("file").unwrap()
            )
        }
        _ => unreachable!(),
    }
}

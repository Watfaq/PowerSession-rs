#[allow(non_snake_case)]
extern crate clap;
extern crate core;

mod commands;
mod terminal;

use clap::{Arg, Command, crate_version};
use commands::{Asciinema, Auth, Play};
use commands::{Record, Upload};
use fern::colors::ColoredLevelConfig;
use log::trace;

fn setup_logger(level: log::LevelFilter) -> Result<(), fern::InitError> {
    let colors = ColoredLevelConfig::new();

    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                colors.color(record.level()),
                message,
            ))
        })
        .level(level)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}

fn main() {
    let app = Command::new("PowerSession")
        .version(crate_version!())
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("rec")
                .about("Record and save a session")
                .arg(
                    Arg::new("file")
                        .help("The filename to save the record")
                        .index(1)
                        .required(true),
                )
                .arg(
                    Arg::new("command")
                        .help("The command to record, defaults to $SHELL")
                        .num_args(1)
                        .short('c')
                        .long("command"),
                )
                .arg(
                    Arg::new("force")
                        .help("Overwrite if session already exists")
                        .num_args(0)
                        .short('f')
                        .long("force"),
                ),
        )
        .subcommand(
            Command::new("play").about("Play a recorded session").arg(
                Arg::new("file")
                    .help("The record session")
                    .index(1)
                    .required(true),
            ),
        )
        .subcommand(
            Command::new("auth").about("Authentication with api server (default is asciinema.org)"),
        )
        .subcommand(
            Command::new("upload")
                .about("Upload a session to api server")
                .arg(
                    Arg::new("file")
                        .help("The file to be uploaded")
                        .index(1)
                        .required(true),
                ),
        )
        .subcommand(
            Command::new("server")
                .about("The url of asciinema server")
                .arg(
                    Arg::new("url")
                        .help("The url of asciinema server. default is https://asciinema.org")
                        .index(1)
                        .required(true),
                ),
        )
        .arg(
            Arg::new("log-level")
                .help("can be one of [error|warn|info|debug|trace]")
                .short('l')
                .long("log-level")
                .default_value("error")
                .default_missing_value("trace")
                .global(true)
                .num_args(1),
        );

    let m = app.get_matches();

    match m.get_one::<String>("log-level") {
        Some(log_level) => match log_level.as_str() {
            "error" => setup_logger(log::LevelFilter::Error).unwrap(),
            "warn" => setup_logger(log::LevelFilter::Warn).unwrap(),
            "info" => setup_logger(log::LevelFilter::Info).unwrap(),
            "debug" => setup_logger(log::LevelFilter::Debug).unwrap(),
            "trace" => setup_logger(log::LevelFilter::Trace).unwrap(),
            _ => unreachable!("unknown log-level"),
        },
        None => setup_logger(log::LevelFilter::Error).unwrap(),
    }

    trace!("PowerSession running");

    match m.subcommand() {
        Some(("play", play_matches)) => {
            let play = Play::new(
                play_matches
                    .get_one::<String>("file")
                    .expect("record file required")
                    .to_owned(),
            );
            play.execute();
        }
        Some(("rec", rec_matches)) => {
            let mut record = Record::new(
                rec_matches.get_one::<String>("file").unwrap().to_owned(),
                None,
                rec_matches.get_one::<String>("command").map(Into::into),
                rec_matches.contains_id("force"),
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
                upload_matches.get_one::<String>("file").unwrap().to_owned(),
            );
            upload.execute();
        }
        Some(("server", new_server)) => {
            let url = &new_server.get_one::<String>("url").unwrap().to_owned();
            let is_url = reqwest::Url::parse(url);
            match is_url {
                Ok(_) => Asciinema::change_server(url.to_string()),
                Err(_) => println!("Error: not a correct URL - e.g: https://asciinema.org"),
            }
        }
        _ => unreachable!(),
    }
}

use super::{ApiService, StreamInfo};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use log::trace;
use os_info::Version;
use platform_dirs::AppDirs;
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(rename = "install_id")]
    install_id: String,
    #[serde(rename = "api_server")]
    api_server: String,
    #[serde(skip)]
    location: String,
}

impl Config {
    fn get_config_file() -> (PathBuf, PathBuf) {
        let app_dirs = AppDirs::new(None, true).unwrap();
        let config_root = app_dirs.config_dir.join("PowerSession");
        let config_file = config_root.join("config.json");

        return (config_root, config_file);
    }

    fn get() -> Self {
        let (_, config_file) = Self::get_config_file();
        return if config_file.exists() {
            let mut c: Config =
                serde_json::from_str(&fs::read_to_string(&config_file).unwrap()).unwrap();
            c.location = config_file.to_str().unwrap().to_owned();
            c
        } else {
            let text = format!(
                "New config file created \nDefault instance will be used: https://asciinema.org \nTo set a custom server type: PowerSession.exe --server <hostname>\n"
            );

            println!("{}", text);
            return Self::new(None);
        };
    }
    fn new(api_server: Option<String>) -> Self {
        let (config_root, config_file) = Self::get_config_file();

        let mut install_id = Uuid::new_v4().to_string();

        if !config_file.exists() {
            fs::create_dir_all(&config_root).unwrap();
            File::create(config_file.to_owned()).unwrap();
        } else {
            install_id = Self::get().install_id;
        }
        // Initialize with default if no value given
        let api_server = api_server.unwrap_or("https://asciinema.org".to_string());
        let c = Config {
            install_id,
            api_server,
            location: config_file.to_str().unwrap().to_owned(),
        };
        let mut f = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(config_file)
            .expect("Failed to create file.");
        f.write_all(serde_json::to_string(&c).unwrap().as_bytes())
            .expect("Failed to write config.");
        return c;
    }

    fn change_api_server(api_server: String) {
        Self::new(Some(api_server.to_owned()));
        let text = format!(
            "Server updated to {api_server}.",
            api_server = api_server.to_owned(),
        );

        println!("{}", text);
    }
}

pub struct Asciinema {
    config: Config,
    http_client: reqwest::blocking::Client,
}

impl Asciinema {
    pub fn new() -> Self {
        let config = Config::get();

        let runtime_version = rustc_version_runtime::version();
        let os_info = os_info::get();
        let (os_major, os_minor, os_build) = match os_info.version() {
            Version::Semantic(major, minor, build) => {
                (major.to_string(), minor.to_string(), build.to_string())
            }
            _ => unreachable!(),
        };

        trace!("rt_info: {}", runtime_version);
        trace!("os_info: {}.{}.{}", os_major, os_minor, os_build);

        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );

        let cred = format!("user:{}", config.install_id);
        let cred_b64 = BASE64_STANDARD.encode(&cred);
        let hdr = format!("Basic {}", cred_b64);
        let mut auth_value = header::HeaderValue::from_str(hdr.as_str()).unwrap();
        auth_value.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, auth_value);

        let client = reqwest::blocking::Client::builder()
            .user_agent(format!(
                "asciinema/2.0.0 rust/{runtime_version} Windows/{os_version_major}-{os_version_major}.{os_version_minor}.{os_version_build}-SP0",
                runtime_version = runtime_version.to_string(),
                os_version_major = os_major,
                os_version_minor = os_minor,
                os_version_build = os_build,
            ))
            .default_headers(headers)
            .build()
            .unwrap();

        Asciinema {
            config,
            http_client: client,
        }
    }
    pub fn change_server(api_server: String) {
        Config::change_api_server(api_server)
    }
}

impl ApiService for Asciinema {
    fn auth(&self) {
        let api_host = &self.config.api_server;
        let auth_url = format!("{}/connect/{}", api_host, self.config.install_id);
        let text = format!(
            "Open the following URL in a web browser to link your \
            install ID with your {api_host} user account:\n\n \
            {auth_url}\n\n \
            This will associate all recordings uploaded from this machine \
            (past and future ones) to your account, \
            and allow you to manage them (change title/theme, delete) at {api_host}.",
            api_host = api_host,
            auth_url = auth_url, // dont know why auto find in scope is not working.
        );

        println!("{}", text);
    }

    fn upload(&self, filepath: &str) -> Option<String> {
        let content = fs::read_to_string(filepath).unwrap();
        let form = reqwest::blocking::multipart::Form::new();
        let part = reqwest::blocking::multipart::Part::text(content)
            .file_name("ascii.cast")
            .mime_str("plain/text")
            .unwrap();

        let form = form.part("asciicast", part);

        let upload_url = format!("{}/api/asciicasts", &self.config.api_server);
        let res = self
            .http_client
            .post(upload_url)
            .multipart(form)
            .send()
            .unwrap();
        if res.status().is_success() {
            Some(
                res.headers()
                    .get(header::LOCATION)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            )
        } else {
            println!("Upload Failed:");
            println!("{}", res.text().unwrap());
            None
        }
    }

    fn create_stream(&self, cols: u16, rows: u16) -> Option<StreamInfo> {
        #[derive(Deserialize)]
        struct CreateStreamResponse {
            id: String,
            url: String,
            ws_producer_url: String,
        }

        let stream_url = format!("{}/api/streams", &self.config.api_server);
        let body = serde_json::json!({ "cols": cols, "rows": rows });
        let res = match self
            .http_client
            .post(stream_url)
            .json(&body)
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                println!("Failed to reach stream server: {}", e);
                return None;
            }
        };

        if res.status().is_success() {
            match res.json::<CreateStreamResponse>() {
                Ok(resp) => Some(StreamInfo {
                    id: resp.id,
                    url: resp.url,
                    ws_producer_url: resp.ws_producer_url,
                }),
                Err(e) => {
                    println!("Failed to parse stream response: {}", e);
                    None
                }
            }
        } else {
            println!("Failed to create stream:");
            println!("{}", res.text().unwrap_or_default());
            None
        }
    }

    fn get_stream_ws_url(&self, stream_id: &str) -> String {
        // Derive the producer WebSocket URL from the server base URL and stream ID.
        // The path follows the asciinema server convention: /ws/S/<stream_id>
        let base = self
            .config
            .api_server
            .trim_end_matches('/')
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        format!("{}/ws/S/{}", base, stream_id)
    }

    fn get_auth_header(&self) -> String {
        let cred = format!("user:{}", self.config.install_id);
        format!("Basic {}", BASE64_STANDARD.encode(&cred))
    }
}

#[cfg(test)]
mod tests {
    use base64::prelude::BASE64_STANDARD;
    use crate::commands::api::asciinema::Config;

    use uuid::{Uuid, Version};

    #[test]
    fn test_config() {
        let c = Config::new(None);
        let uuid = Uuid::parse_str(&c.install_id);
        assert_eq!(uuid.unwrap().get_version(), Some(Version::Random)); // uuid4
    }

    #[test]
    fn test_get_stream_ws_url_https_and_http() {
        let client = reqwest::blocking::Client::new();

        let asc_https = super::Asciinema {
            config: Config {
                install_id: "install".to_string(),
                api_server: "https://demo.asciinema.org/".to_string(),
                location: String::new(),
            },
            http_client: client.clone(),
        };
        assert_eq!(
            asc_https.get_stream_ws_url("abc123"),
            "wss://demo.asciinema.org/ws/S/abc123"
        );

        let asc_http = super::Asciinema {
            config: Config {
                install_id: "install".to_string(),
                api_server: "http://asciinema.test".to_string(),
                location: String::new(),
            },
            http_client: client,
        };
        assert_eq!(
            asc_http.get_stream_ws_url("xyz"),
            "ws://asciinema.test/ws/S/xyz"
        );
    }

    #[test]
    fn test_get_auth_header_format() {
        let asc = super::Asciinema {
            config: Config {
                install_id: "token-123".to_string(),
                api_server: "https://example".to_string(),
                location: String::new(),
            },
            http_client: reqwest::blocking::Client::new(),
        };

        let expected = format!("Basic {}", BASE64_STANDARD.encode("user:token-123"));
        assert_eq!(asc.get_auth_header(), expected);
    }
}

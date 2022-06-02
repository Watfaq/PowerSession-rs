use super::ApiService;

use platform_dirs::AppDirs;
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::Write;

use uuid::Uuid;

#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(rename = "install_id")]
    install_id: String,
    #[serde(skip)]
    location: String,
}

impl Config {
    fn new() -> Self {
        let app_dirs = AppDirs::new(None, true).unwrap();
        let config_root = app_dirs.config_dir.join("PowerSession");

        fs::create_dir_all(&config_root).unwrap();

        let config_file = config_root.join("config.json");
        return if config_file.exists() {
            let mut c: Config =
                serde_json::from_str(&fs::read_to_string(&config_file).unwrap()).unwrap();
            c.location = config_file.to_str().unwrap().to_owned();
            c
        } else {
            let install_id = Uuid::new_v4().to_string();
            let c = Config {
                install_id,
                location: config_file.to_str().unwrap().to_owned(),
            };
            let mut f = File::create(config_file).unwrap();
            f.write_all(serde_json::to_string(&c).unwrap().as_bytes())
                .unwrap();
            c
        };
    }
}

pub struct Asciinema {
    config: Config,
    api_host: String,
    http_client: reqwest::blocking::Client,
}

impl Asciinema {
    pub fn new() -> Self {
        let config = Config::new();

        let runtime_version = rustc_version_runtime::version();
        let os_info = os_info::get();

        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );

        let cred = format!("user:{}", config.install_id);
        let cred_b64 = base64::encode_config(cred, base64::STANDARD);
        let hdr = format!("Basic {}", cred_b64);
        let mut auth_value = header::HeaderValue::from_str(hdr.as_str()).unwrap();
        auth_value.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, auth_value);

        let client = reqwest::blocking::Client::builder()
            .user_agent(format!(
                "asciinema/2.0.0 {runtime_version} {os_info}",
                runtime_version = runtime_version.to_string(),
                os_info = os_info.to_string(),
            ))
            .default_headers(headers)
            .build()
            .unwrap();

        Asciinema {
            config,
            api_host: "https://asciinema.org".to_owned(),
            http_client: client,
        }
    }
}

impl ApiService for Asciinema {
    fn auth(&self) {
        let api_host = &self.api_host;
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

        let upload_url = format!("{}/api/asciicasts", self.api_host);
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
}

#[cfg(test)]
mod tests {
    use crate::commands::api::asciinema::Config;
    
    
    use uuid::{Uuid, Version};

    #[test]
    fn test_config() {
        let c = Config::new();
        let uuid = Uuid::parse_str(&c.install_id);
        assert_eq!(uuid.unwrap().get_version(), Some(Version::Random)); // uuid4
    }
}

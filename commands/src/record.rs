use std::{collections::HashMap, env, fs, path::Path};

struct RecordHeader {
    version: u8,
    width: u32,
    height: u32,
    timestamp: u64,
    env: HashMap<&'static str, &'static str>,
}

pub struct Record {
    filename: &'static str,
    env: Option<HashMap<&'static str, &'static str>>,
    command: &'static str,

    overwrite: bool,
}

impl Record {
    pub fn execute(&mut self) {
        assert_eq!(self.filename.is_empty(), false);

        if Path::new(self.filename).exists() {
            if self.overwrite {
                fs::remove_file(self.filename);
            }
        }

        let env = self.env.get_or_insert(HashMap::new());

        env.insert("POWERSESSION_RECORDING", "1");
        env.insert("SHELL", "powershell.exe");
        let term: String = env::var("TERMINAL_EMULATOR").unwrap_or("UnKnown".to_string());
        env.insert("TERM", &term);

        self.record();
    }

    fn record(&self) {}
}

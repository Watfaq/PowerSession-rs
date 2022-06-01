mod asciinema;

pub trait ApiService {
    fn auth(&self);
    fn upload(&self, filepath: &str) -> Option<String>;
}

pub use asciinema::Asciinema;

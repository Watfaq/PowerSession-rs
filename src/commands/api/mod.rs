mod asciinema;

pub struct StreamInfo {
    pub id: String,
    pub url: String,
    pub ws_producer_url: String,
}

pub trait ApiService {
    fn auth(&self);
    fn upload(&self, filepath: &str) -> Option<String>;
    /// Create a new live stream on the server. Returns `StreamInfo` with the public viewer
    /// URL and the WebSocket producer URL to push events to.
    fn create_stream(&self, cols: u16, rows: u16) -> Option<StreamInfo>;
    /// Build the WebSocket producer URL for an *existing* stream identified by `stream_id`.
    fn get_stream_ws_url(&self, stream_id: &str) -> String;
    /// Return the `Authorization` header value to authenticate WebSocket connections.
    fn get_auth_header(&self) -> String;
}

pub use asciinema::Asciinema;

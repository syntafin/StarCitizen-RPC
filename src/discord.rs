use crate::state::RenderedPresence;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::time::{Duration, Instant};

const RECONNECT_BACKOFF: Duration = Duration::from_secs(5);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

pub struct DiscordClient {
    app_id: String,
    client: Option<DiscordIpcClient>,
    connected: bool,
    last_sent: Option<RenderedPresence>,
    last_connect_attempt: Option<Instant>,
    last_heartbeat: Option<Instant>,
}

impl DiscordClient {
    pub fn new(app_id: String) -> Self {
        Self {
            app_id,
            client: None,
            connected: false,
            last_sent: None,
            last_connect_attempt: None,
            last_heartbeat: None,
        }
    }

    fn ensure_connected(&mut self) -> bool {
        if self.connected {
            return true;
        }

        if let Some(last) = self.last_connect_attempt {
            if last.elapsed() < RECONNECT_BACKOFF {
                return false;
            }
        }
        self.last_connect_attempt = Some(Instant::now());

        let mut client = match DiscordIpcClient::new(&self.app_id) {
            Ok(c) => c,
            Err(err) => {
                eprintln!("Discord Client Init fehlgeschlagen: {err}");
                return false;
            }
        };

        match client.connect() {
            Ok(()) => {
                println!("Mit Discord RPC verbunden (App-ID: {}).", self.app_id);
                self.client = Some(client);
                self.connected = true;
                self.last_sent = None;
                self.last_heartbeat = None;
                true
            }
            Err(_) => false,
        }
    }

    pub fn update(&mut self, presence: &RenderedPresence) {
        let needs_heartbeat = self
            .last_heartbeat
            .is_none_or(|t| t.elapsed() >= HEARTBEAT_INTERVAL);

        if self.last_sent.as_ref() == Some(presence) && !needs_heartbeat {
            return;
        }
        if !self.ensure_connected() {
            return;
        }

        let mut assets = activity::Assets::new()
            .large_image(&presence.large_image)
            .large_text(&presence.large_text);

        if let (Some(small_img), Some(small_txt)) = (&presence.small_image, &presence.small_text) {
            assets = assets.small_image(small_img).small_text(small_txt);
        }

        let mut act = activity::Activity::new()
            .details(&presence.details)
            .state(&presence.state)
            .assets(assets);

        if let Some(ts) = presence.start_timestamp {
            act = act.timestamps(activity::Timestamps::new().start(ts));
        }

        if let Some(client) = &mut self.client {
            match client.set_activity(act) {
                Ok(()) => {
                    if self.last_sent.as_ref() != Some(presence) {
                        println!("Presence aktualisiert: {presence}");
                    }
                    self.last_sent = Some(presence.clone());
                    self.last_heartbeat = Some(Instant::now());
                }
                Err(err) => {
                    eprintln!("Discord Verbindung verloren ({err}), versuche Reconnect...");
                    self.connected = false;
                    self.client = None;
                    self.last_sent = None;
                    self.last_heartbeat = None;
                }
            }
        }
    }

    pub fn clear(&mut self) {
        if self.last_sent.is_none() && !self.connected {
            return;
        }
        if let Some(client) = &mut self.client {
            if client.clear_activity().is_err() {
                self.connected = false;
                self.client = None;
            }
        }
        self.last_sent = None;
        self.last_heartbeat = None;
    }

    pub fn close(&mut self) {
        if let Some(mut client) = self.client.take() {
            let _ = client.clear_activity();
            let _ = client.close();
        }
        self.connected = false;
        self.last_sent = None;
        self.last_heartbeat = None;
    }
}

//! Finds connected cameras, keeps one of them open (C5), and notices when
//! cameras come and go (C6).

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use super::worker::{Command, Event, Waker, Worker};
use super::{Backend, CameraInfo, CameraSettings, Capture, Control, Setting};

/// How often the connected cameras are listed.
const DETECT_INTERVAL: Duration = Duration::from_secs(2);
/// How long to wait before opening a camera again after it failed to open.
const RETRY_INTERVAL: Duration = Duration::from_secs(3);

/// The state of the camera connection, shown in the status bar (U3, C6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    None,
    Connecting(CameraInfo),
    Connected(CameraInfo),
    /// The camera failed to open or was disconnected, with the reason.
    Lost(CameraInfo, String),
}

/// Something the rest of the program needs to act on.
pub enum CameraEvent {
    /// A camera was opened. Its settings need to be set for the active shot
    /// (CT7).
    Connected,
    /// The camera accepted a new value for a setting.
    SettingChanged(Setting, String),
    Captured(Capture),
    /// Something failed. The message is shown to the user (U5).
    Error(String),
}

type Backends = Arc<Mutex<Vec<Box<dyn Backend>>>>;

pub struct Cameras {
    backends: Backends,
    detected: Receiver<Vec<(usize, CameraInfo)>>,
    /// Connected cameras with the index of their backend.
    available: Vec<(usize, CameraInfo)>,
    wake: Waker,
    worker: Option<Worker>,
    connection: Connection,
    controls: Vec<Control>,
    settings: CameraSettings,
    /// The camera to connect to when it is available (C5).
    preferred: Option<String>,
    retry_at: Option<Instant>,
    capturing: bool,
    live_view: bool,
    live_sequence: u64,
    live_texture: Option<egui::TextureHandle>,
}

impl Cameras {
    /// Starts looking for cameras. `preferred` is the last used camera, which
    /// is opened as soon as it is found (C5).
    pub fn new(ctx: &egui::Context, preferred: Option<String>) -> Self {
        let repaint = ctx.clone();
        let wake: Waker = Arc::new(move || repaint.request_repaint());
        let backends: Backends = Arc::new(Mutex::new(super::backends()));
        let (sender, detected) = mpsc::channel();
        let detect_backends = Arc::clone(&backends);
        let detect_wake = Arc::clone(&wake);
        thread::spawn(move || {
            let mut last = None;
            loop {
                let found = detect(&detect_backends);
                if last.as_ref() != Some(&found) {
                    if sender.send(found.clone()).is_err() {
                        return;
                    }
                    detect_wake();
                    last = Some(found);
                }
                thread::sleep(DETECT_INTERVAL);
            }
        });
        Self {
            backends,
            detected,
            available: Vec::new(),
            wake,
            worker: None,
            connection: Connection::None,
            controls: Vec::new(),
            settings: CameraSettings::default(),
            preferred,
            retry_at: None,
            capturing: false,
            live_view: false,
            live_sequence: 0,
            live_texture: None,
        }
    }

    /// Handles everything that happened since the last call. Called once per
    /// UI frame.
    pub fn poll(&mut self) -> Vec<CameraEvent> {
        let mut events = Vec::new();
        if let Some(found) = self.detected.try_iter().last() {
            self.available = found;
        }
        while let Some(event) = self.worker.as_ref().and_then(Worker::try_event) {
            self.handle(event, &mut events);
        }
        self.reconnect();
        events
    }

    /// Cameras that can be picked.
    pub fn available(&self) -> impl Iterator<Item = &CameraInfo> {
        self.available.iter().map(|(_, info)| info)
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.connection, Connection::Connected(_))
    }

    /// The settings the open camera supports.
    pub fn controls(&self) -> &[Control] {
        &self.controls
    }

    /// The open camera's current settings.
    pub fn settings(&self) -> &CameraSettings {
        &self.settings
    }

    /// Opens the camera with `id` in place of the open one and remembers it as
    /// the camera to reconnect to (C5).
    pub fn connect(&mut self, id: &str) {
        self.preferred = Some(id.to_owned());
        if self.current().is_some_and(|info| info.id == id) && self.worker.is_some() {
            return;
        }
        self.close(Connection::None);
        self.retry_at = None;
        self.reconnect();
    }

    /// Asks the camera to change a setting. The result arrives as a
    /// [`CameraEvent`].
    pub fn set(&self, setting: Setting, value: &str) {
        if let Some(worker) = self.connected_worker() {
            worker.send(Command::Set(setting, value.to_owned()));
        }
    }

    /// Turns live view on while a tab shows it, and off otherwise.
    pub fn set_live_view(&mut self, on: bool) {
        if self.live_view != on {
            self.live_view = on;
            if let Some(worker) = &self.worker {
                worker.send(Command::LiveView(on));
            }
        }
    }

    /// Starts a capture unless one is already running. Returns whether it
    /// started.
    pub fn capture(&mut self) -> bool {
        if self.capturing {
            return false;
        }
        let Some(worker) = self.connected_worker() else {
            return false;
        };
        worker.send(Command::Capture);
        self.capturing = true;
        true
    }

    pub fn is_capturing(&self) -> bool {
        self.capturing
    }

    /// Waits up to `timeout` for a running capture to finish, so it can be
    /// written before the project closes (F9). Other events that arrive in
    /// the meantime are returned too.
    pub fn finish_capture(&mut self, timeout: Duration) -> Vec<CameraEvent> {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        while self.capturing {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(event) = self.worker.as_ref().and_then(|w| w.event_timeout(left)) else {
                break;
            };
            self.handle(event, &mut events);
        }
        events
    }

    /// The newest live view frame as a texture.
    pub fn live_texture(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        let worker = self.connected_worker()?;
        if let Some((sequence, frame)) = worker.live_frame(self.live_sequence) {
            self.live_sequence = sequence;
            let size = [frame.width as usize, frame.height as usize];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &frame.rgba);
            match &mut self.live_texture {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => {
                    self.live_texture =
                        Some(ctx.load_texture("live_view", image, egui::TextureOptions::LINEAR));
                }
            }
        }
        self.live_texture.as_ref()
    }

    fn connected_worker(&self) -> Option<&Worker> {
        if self.is_connected() {
            self.worker.as_ref()
        } else {
            None
        }
    }

    fn current(&self) -> Option<&CameraInfo> {
        match &self.connection {
            Connection::None => None,
            Connection::Connecting(info)
            | Connection::Connected(info)
            | Connection::Lost(info, _) => Some(info),
        }
    }

    fn handle(&mut self, event: Event, events: &mut Vec<CameraEvent>) {
        match event {
            Event::Opened { controls, settings } => {
                self.controls = controls;
                self.settings = settings;
                if let Connection::Connecting(info) = &self.connection {
                    self.connection = Connection::Connected(info.clone());
                }
                if self.live_view
                    && let Some(worker) = &self.worker
                {
                    worker.send(Command::LiveView(true));
                }
                events.push(CameraEvent::Connected);
            }
            Event::Set {
                setting,
                value,
                result,
                settings,
            } => {
                self.settings = settings;
                match result {
                    Ok(()) => events.push(CameraEvent::SettingChanged(setting, value)),
                    Err(e) => events.push(CameraEvent::Error(format!(
                        "Could not set {} to {value}: {e}",
                        setting.label()
                    ))),
                }
            }
            Event::Captured(result) => {
                self.capturing = false;
                match result {
                    Ok(capture) => events.push(CameraEvent::Captured(capture)),
                    Err(e) => events.push(CameraEvent::Error(format!("The capture failed: {e}"))),
                }
            }
            Event::Lost(error) => {
                if self.capturing {
                    events.push(CameraEvent::Error(format!("The capture failed: {error}")));
                }
                let info = self.current().cloned();
                self.close(Connection::None);
                if let Some(info) = info {
                    self.connection = Connection::Lost(info, error.to_string());
                }
                self.retry_at = Some(Instant::now() + RETRY_INTERVAL);
            }
        }
    }

    /// Opens the preferred camera if nothing is open and it is connected.
    fn reconnect(&mut self) {
        if self.worker.is_some() || self.retry_at.is_some_and(|at| Instant::now() < at) {
            return;
        }
        let Some(preferred) = &self.preferred else {
            return;
        };
        let Some((backend, info)) = self
            .available
            .iter()
            .find(|(_, info)| &info.id == preferred)
            .cloned()
        else {
            return;
        };
        self.retry_at = None;
        let backends = Arc::clone(&self.backends);
        let id = info.id.clone();
        let open = Box::new(move || {
            let mut backends = backends
                .lock()
                .map_err(|_| super::Error::Failed("The camera backend stopped.".into()))?;
            backends[backend].open(&id)
        });
        self.worker = Some(Worker::spawn(open, Arc::clone(&self.wake)));
        self.connection = Connection::Connecting(info);
    }

    /// Drops the open camera, stopping its worker.
    fn close(&mut self, connection: Connection) {
        self.worker = None;
        self.connection = connection;
        self.controls.clear();
        self.settings = CameraSettings::default();
        self.capturing = false;
        self.live_sequence = 0;
        self.live_texture = None;
    }
}

fn detect(backends: &Backends) -> Vec<(usize, CameraInfo)> {
    let Ok(mut backends) = backends.lock() else {
        return Vec::new();
    };
    backends
        .iter_mut()
        .enumerate()
        .flat_map(|(index, backend)| backend.detect().into_iter().map(move |info| (index, info)))
        .collect()
}

//! Runs one open camera on its own thread so live view, capture, and
//! settings changes never block the UI (AT12).

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

use super::{Camera, CameraSettings, Capture, Control, Error, Frame, Result, Setting};

/// How long to wait after a failed live view frame before asking again.
const RETRY_FRAME: std::time::Duration = std::time::Duration::from_millis(50);

/// Asks the UI to redraw, for example when a live view frame arrives.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Opens the camera on the worker thread, since opening can be slow.
pub type Opener = Box<dyn FnOnce() -> Result<Box<dyn Camera>> + Send>;

pub enum Command {
    Set(Setting, String),
    LiveView(bool),
    Capture,
}

pub enum Event {
    Opened {
        controls: Vec<Control>,
        settings: CameraSettings,
    },
    Set {
        setting: Setting,
        value: String,
        result: Result<()>,
        settings: CameraSettings,
    },
    Captured(Result<Capture>),
    /// The camera could not be opened or was disconnected. The worker has
    /// stopped.
    Lost(Error),
}

/// The most recent live view frame. Older frames are dropped, so a slow UI
/// always shows the newest one.
#[derive(Default)]
pub struct LiveFrame {
    /// Increases with every new frame.
    pub sequence: u64,
    pub frame: Option<Arc<Frame>>,
}

/// The UI side of a camera worker. Dropping it stops the worker.
pub struct Worker {
    commands: Sender<Command>,
    events: Receiver<Event>,
    live: Arc<Mutex<LiveFrame>>,
}

impl Worker {
    pub fn spawn(open: Opener, wake: Waker) -> Self {
        let (commands, command_rx) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let live = Arc::new(Mutex::new(LiveFrame::default()));
        let shared = Arc::clone(&live);
        thread::spawn(move || run(open, &command_rx, &event_tx, &shared, &wake));
        Self {
            commands,
            events,
            live,
        }
    }

    pub fn send(&self, command: Command) {
        // A stopped worker has already reported why with `Event::Lost`.
        let _ = self.commands.send(command);
    }

    pub fn try_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    pub fn event_timeout(&self, timeout: std::time::Duration) -> Option<Event> {
        self.events.recv_timeout(timeout).ok()
    }

    /// The newest live view frame if it is newer than `seen`.
    pub fn live_frame(&self, seen: u64) -> Option<(u64, Arc<Frame>)> {
        let live = self.live.lock().ok()?;
        if live.sequence == seen {
            return None;
        }
        Some((live.sequence, Arc::clone(live.frame.as_ref()?)))
    }
}

fn run(
    open: Opener,
    commands: &Receiver<Command>,
    events: &Sender<Event>,
    live: &Mutex<LiveFrame>,
    wake: &Waker,
) {
    let send = |event| {
        let _ = events.send(event);
        wake();
    };
    let mut camera = match open() {
        Ok(camera) => camera,
        Err(e) => return send(Event::Lost(e)),
    };
    send(Event::Opened {
        controls: camera.controls(),
        settings: camera.settings(),
    });

    let mut live_view = false;
    loop {
        // Commands come first so a capture never waits behind live view.
        let command = if live_view {
            match commands.try_recv() {
                Ok(command) => Some(command),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        } else {
            match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        };

        let lost = match command {
            Some(Command::Set(setting, value)) => {
                let result = camera.set(setting, &value);
                let lost = disconnected(&result);
                send(Event::Set {
                    setting,
                    value,
                    result,
                    settings: camera.settings(),
                });
                lost
            }
            Some(Command::LiveView(on)) => {
                let result = camera.set_live_view(on);
                live_view = on && result.is_ok();
                match result {
                    Err(Error::Disconnected(e)) => Some(Error::Disconnected(e)),
                    // Without live view the camera can still capture.
                    _ => None,
                }
            }
            Some(Command::Capture) => {
                let result = camera.capture();
                let lost = disconnected(&result);
                send(Event::Captured(result));
                lost
            }
            None => match camera.live_view() {
                Ok(frame) => {
                    if let Ok(mut slot) = live.lock() {
                        slot.sequence += 1;
                        slot.frame = Some(Arc::new(frame));
                    }
                    wake();
                    None
                }
                Err(Error::Disconnected(e)) => Some(Error::Disconnected(e)),
                // A dropped frame is not worth reporting; try again shortly.
                Err(Error::Failed(_)) => {
                    thread::sleep(RETRY_FRAME);
                    None
                }
            },
        };
        if let Some(error) = lost {
            return send(Event::Lost(error));
        }
    }
    let _ = camera.set_live_view(false);
}

fn disconnected<T>(result: &Result<T>) -> Option<Error> {
    match result {
        Err(Error::Disconnected(e)) => Some(Error::Disconnected(e.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// A camera that unplugs itself after a few live view frames.
    struct Unplugged {
        frames: u32,
    }

    impl Camera for Unplugged {
        fn controls(&self) -> Vec<Control> {
            Vec::new()
        }

        fn settings(&self) -> CameraSettings {
            CameraSettings::default()
        }

        fn set(&mut self, _: Setting, _: &str) -> Result<()> {
            Err(Error::Failed("read only".into()))
        }

        fn set_live_view(&mut self, _: bool) -> Result<()> {
            Ok(())
        }

        fn live_view(&mut self) -> Result<Frame> {
            if self.frames == 0 {
                return Err(Error::Disconnected("unplugged".into()));
            }
            self.frames -= 1;
            Ok(Frame {
                width: 1,
                height: 1,
                rgba: vec![0; 4],
            })
        }

        fn capture(&mut self) -> Result<Capture> {
            Err(Error::Failed("no capture".into()))
        }
    }

    fn next(worker: &Worker) -> Event {
        worker.event_timeout(Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn disconnect_stops_the_worker() {
        let worker = Worker::spawn(
            Box::new(|| Ok(Box::new(Unplugged { frames: 3 }) as Box<dyn Camera>)),
            Arc::new(|| {}),
        );
        assert!(matches!(next(&worker), Event::Opened { .. }));

        worker.send(Command::Capture);
        assert!(matches!(
            next(&worker),
            Event::Captured(Err(Error::Failed(_)))
        ));
        worker.send(Command::Set(Setting::Iso, "100".into()));
        assert!(matches!(next(&worker), Event::Set { result: Err(_), .. }));

        worker.send(Command::LiveView(true));
        assert!(matches!(next(&worker), Event::Lost(Error::Disconnected(_))));
        assert_eq!(worker.live_frame(0).map(|(seq, _)| seq), Some(3));
    }

    #[test]
    fn open_failure_is_reported() {
        let worker = Worker::spawn(
            Box::new(|| Err(Error::Failed("busy".into()))),
            Arc::new(|| {}),
        );
        assert!(matches!(next(&worker), Event::Lost(Error::Failed(_))));
    }
}

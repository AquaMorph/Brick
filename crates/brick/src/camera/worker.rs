//! Runs one open camera on its own thread so live view, capture, and
//! settings changes never block the UI (AT12).

use std::io::Cursor;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

use image::codecs::jpeg::JpegEncoder;

use super::{Camera, CameraSettings, Capture, Control, Error, Frame, Result, Setting};

/// How long to wait after a failed live view frame before asking again.
const RETRY_FRAME: std::time::Duration = std::time::Duration::from_millis(50);

/// Longest side of a preview made from the captured image when no live view
/// frame is available.
const PREVIEW_SIDE: u32 = 960;
const PREVIEW_QUALITY: u8 = 90;

/// Asks the UI to redraw, for example when a live view frame arrives.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Opens the camera on the worker thread, since opening can be slow.
pub type Opener = Box<dyn FnOnce() -> Result<Box<dyn Camera>> + Send>;

pub enum Command {
    Set(Setting, String),
    LiveView(bool),
    /// Takes a picture. With `preview`, the live view frame at the moment of
    /// capture is kept as a low resolution JPEG (P14).
    Capture {
        preview: bool,
    },
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
    Captured {
        result: Result<Capture>,
        /// The camera's settings when the picture was taken.
        settings: CameraSettings,
        /// The low resolution preview, when asked for and it could be made.
        preview: Option<Vec<u8>>,
    },
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
            Some(Command::Capture { preview }) => {
                let settings = camera.settings();
                // The newest live view frame is what the animator saw when
                // they asked for the capture.
                let live = live_view.then(|| live.lock().ok()?.frame.clone()).flatten();
                let result = camera.capture();
                let lost = disconnected(&result);
                let preview = match (&result, preview) {
                    (Ok(capture), true) => make_preview(live.as_deref(), capture),
                    _ => None,
                };
                send(Event::Captured {
                    result,
                    settings,
                    preview,
                });
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

/// The low resolution preview of a capture as a JPEG: the live view frame
/// grabbed at the moment of capture, or the captured image scaled down when
/// live view was off (P14).
fn make_preview(live: Option<&Frame>, capture: &Capture) -> Option<Vec<u8>> {
    let rgb = match live {
        Some(frame) => {
            let rgba = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())?;
            image::DynamicImage::ImageRgba8(rgba).into_rgb8()
        }
        None => image::load_from_memory(&capture.jpeg)
            .ok()?
            .thumbnail(PREVIEW_SIDE, PREVIEW_SIDE)
            .into_rgb8(),
    };
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut jpeg), PREVIEW_QUALITY)
        .encode_image(&rgb)
        .ok()?;
    Some(jpeg)
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

        worker.send(Command::Capture { preview: true });
        assert!(matches!(
            next(&worker),
            Event::Captured {
                result: Err(Error::Failed(_)),
                ..
            }
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

#[cfg(test)]
mod preview_tests {
    use super::*;

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let rgb = image::RgbImage::new(width, height);
        let mut jpeg = Vec::new();
        JpegEncoder::new(Cursor::new(&mut jpeg))
            .encode_image(&rgb)
            .unwrap();
        jpeg
    }

    fn size(jpeg: &[u8]) -> (u32, u32) {
        let image = image::load_from_memory(jpeg).unwrap();
        (image.width(), image.height())
    }

    #[test]
    fn preview_is_the_live_view_frame() {
        let capture = Capture {
            jpeg: jpeg(2000, 1000),
            raw: None,
        };
        let live = Frame {
            width: 64,
            height: 48,
            rgba: vec![255; 64 * 48 * 4],
        };
        assert_eq!(
            size(&make_preview(Some(&live), &capture).unwrap()),
            (64, 48)
        );
        // Without live view the capture is scaled down instead.
        assert_eq!(
            size(&make_preview(None, &capture).unwrap()),
            (PREVIEW_SIDE, PREVIEW_SIDE / 2)
        );
    }
}

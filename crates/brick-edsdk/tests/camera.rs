//! Drives a connected Canon camera through everything Brick uses: settings,
//! live view, and a capture. It only runs when asked, with a camera plugged
//! in and turned on:
//!
//! ```sh
//! BRICK_EDSDK_CAMERA_TEST=1 cargo test -p brick-edsdk --test camera
//! ```
//!
//! Set `BRICK_EDSDK_TEST_OUT` to a directory to keep the captured files.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use brick_edsdk::{Camera, Property};

fn main() {
    if std::env::var_os("BRICK_EDSDK_CAMERA_TEST").is_none() {
        println!("skipped: set BRICK_EDSDK_CAMERA_TEST=1 with a Canon camera connected");
        return;
    }
    if !brick_edsdk::AVAILABLE {
        println!("skipped: built without the EDSDK");
        return;
    }
    // The camera is driven from another thread, as in the program, while
    // this thread runs the main run loop the SDK needs on macOS.
    let (done, finished) = mpsc::channel();
    let test = thread::spawn(move || {
        connected_camera();
        let _ = done.send(());
    });
    while finished.try_recv() == Err(mpsc::TryRecvError::Empty) && !test.is_finished() {
        run_main_loop();
    }
    if test.join().is_err() {
        std::process::exit(1);
    }
    println!("camera test passed");
}

#[cfg(target_os = "macos")]
fn run_main_loop() {
    use std::ffi::c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, after_source: u8) -> i32;
    }
    // SAFETY: Runs the main thread's run loop for a moment, in the default
    // mode, from the main thread.
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, 0);
    }
}

#[cfg(not(target_os = "macos"))]
fn run_main_loop() {
    thread::sleep(Duration::from_millis(50));
}

fn connected_camera() {
    let started = Instant::now();
    let mut devices = brick_edsdk::cameras().unwrap();
    while devices.is_empty() && started.elapsed() < Duration::from_secs(10) {
        thread::sleep(Duration::from_millis(250));
        devices = brick_edsdk::cameras().unwrap();
    }
    println!("cameras after {:?}: {devices:?}", started.elapsed());
    let device = devices.first().expect("no Canon camera connected");
    let camera = Camera::open(&device.port).unwrap();
    for property in [
        Property::ImageQuality,
        Property::AeMode,
        Property::Tv,
        Property::Av,
        Property::Iso,
    ] {
        let value = camera.get(property);
        let choices = camera.choices(property);
        println!("{property:?}: {value:x?} of {choices:x?}");
    }

    camera.set_live_view(true).unwrap();
    let mut frames = 0;
    let started = Instant::now();
    while frames < 30 && started.elapsed() < Duration::from_secs(10) {
        match camera.live_view_frame().unwrap() {
            Some(jpeg) => {
                assert_eq!(jpeg[..2], [0xFF, 0xD8], "live view is not a JPEG");
                frames += 1;
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
    println!("{frames} live view frames in {:?}", started.elapsed());
    assert_eq!(frames, 30);

    // The second image of an image quality has format 0 when there is none.
    let quality = camera.get(Property::ImageQuality).unwrap();
    let count = if quality & 0xF0 == 0 { 1 } else { 2 };
    let started = Instant::now();
    let files = camera.capture(count, Duration::from_secs(60)).unwrap();
    println!("captured in {:?}", started.elapsed());
    for file in &files {
        println!("{} ({} bytes)", file.name, file.data.len());
        if let Some(dir) = std::env::var_os("BRICK_EDSDK_TEST_OUT") {
            std::fs::write(std::path::Path::new(&dir).join(&file.name), &file.data).unwrap();
        }
    }
    assert_eq!(files.len(), count);
    camera.set_live_view(false).unwrap();
}

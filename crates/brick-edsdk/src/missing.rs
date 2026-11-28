//! Stands in for the SDK when Brick is built without it (R11). Nothing can
//! be opened, so every call fails.

use std::time::Duration;

use crate::{DeviceInfo, Error, File, Result};

pub fn cameras() -> Result<Vec<DeviceInfo>> {
    Err(Error::NotInstalled)
}

pub fn open(_port: &str) -> Result<u64> {
    Err(Error::NotInstalled)
}

pub fn close(_camera: u64) {}

pub fn get(_camera: u64, _property: u32) -> Result<u32> {
    Err(Error::NotInstalled)
}

pub fn set(_camera: u64, _property: u32, _value: u32) -> Result<()> {
    Err(Error::NotInstalled)
}

pub fn choices(_camera: u64, _property: u32) -> Result<Vec<u32>> {
    Err(Error::NotInstalled)
}

pub fn set_live_view(_camera: u64, _on: bool) -> Result<()> {
    Err(Error::NotInstalled)
}

pub fn live_view_frame(_camera: u64) -> Result<Option<Vec<u8>>> {
    Err(Error::NotInstalled)
}

pub fn capture(_camera: u64, _files: usize, _timeout: Duration) -> Result<Vec<File>> {
    Err(Error::NotInstalled)
}

use std::os::fd::{IntoRawFd, OwnedFd, RawFd};

pub struct Object {
    pub width: u32,
    pub height: u32,
    pub num_objects: u32,
    pub format: u32,
    pub fds: Vec<RawFd>,
    pub sizes: Vec<u32>,
}

impl Object {
    pub fn new(width: u32, height: u32, num_objects: u32, format: u32) -> Self {
        Self {
            width,
            height,
            num_objects,
            format,
            fds: vec![0; num_objects as usize],
            sizes: vec![0; num_objects as usize],
        }
    }

    pub fn set_object(&mut self, index: u32, fd: OwnedFd, size: u32) {
        self.fds[index as usize] = fd.into_raw_fd();
        self.sizes[index as usize] = size;
    }
}

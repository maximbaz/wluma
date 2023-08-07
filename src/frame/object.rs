use std::collections::HashMap;
use wayland_backend::io_lifetimes::OwnedFd;

#[derive(Default)]
pub struct Object {
    pub width: u32,
    pub height: u32,
    pub num_objects: u32,
    pub fds: HashMap<u32, OwnedFd>,
    pub sizes: HashMap<u32, u32>,
}

impl Object {
    pub fn set_metadata(&mut self, width: u32, height: u32, num_objects: u32) {
        self.width = width;
        self.height = height;
        self.num_objects = num_objects;
        self.fds = HashMap::new();
        self.sizes = HashMap::new();
    }

    pub fn set_object(&mut self, index: u32, fd: OwnedFd, size: u32) {
        self.fds.insert(index, fd);
        self.sizes.insert(index, size);
    }
}

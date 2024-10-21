use std::os::unix::io::RawFd;

#[derive(Default)]
pub struct Object {
    pub width: u32,
    pub height: u32,
    pub num_objects: u32,
    pub format: u32,
    pub fds: Vec<RawFd>,
    pub sizes: Vec<u32>,
}

impl Object {
    pub fn set_metadata(&mut self, width: u32, height: u32, num_objects: u32, format: u32) {
        self.width = width;
        self.height = height;
        self.num_objects = num_objects;
        self.format = format;
        self.fds.resize(num_objects as usize, 0);
        self.sizes.resize(num_objects as usize, 0);
    }

    pub fn set_object(&mut self, index: u32, fd: RawFd, size: u32) {
        self.fds[index as usize] = fd;
        self.sizes[index as usize] = size;
    }
}

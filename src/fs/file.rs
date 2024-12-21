use std:: os::fd::{AsRawFd, IntoRawFd, RawFd};

use crate::{
    io::{AsyncReader, AsyncWriter},
    reactor::get_reactor
};

const AT_FDWCD: isize = -100;

pub struct File {
    fd: RawFd,
}

impl File {
    pub fn open(path: &str) -> Self {
        let path = std::ffi::CString::new(path).unwrap();
        // println!("path: {:?}", path.as_c_str().to_bytes());
        unsafe {
            let fd = libc::openat(AT_FDWCD as i32, path.as_ptr() as *const _, libc::O_RDONLY);

            Self { fd }
        }
    }

    pub fn read<'a>(&'a self, buf: &'a mut [u8]) -> AsyncReader<'a> {
        AsyncReader::new(self.fd, buf)
    }

    pub fn write<'a>(&'a self, buf: &'a [u8]) -> AsyncWriter<'a> {
        AsyncWriter::new(self.fd, buf)
    }
}

impl From<std::fs::File> for File {
    fn from(file: std::fs::File) -> Self {
        let fd = file.into_raw_fd();
        Self { fd }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        unsafe {
            let reactor = get_reactor();
            reactor.borrow_mut().unregister_fd(self.fd);

            libc::close(self.fd);
        }
    }
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}
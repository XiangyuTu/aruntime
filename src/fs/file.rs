use std::{
    cell::RefCell,
    os::fd::{AsRawFd, IntoRawFd, RawFd},
    rc::{Rc, Weak},
};

use crate::{
    io::{AsyncReader, AsyncWriter},
    reactor::{get_reactor, Reactor},
};

const AT_FDWCD: isize = -100;

pub struct File {
    fd: RawFd,

    reactor: Weak<RefCell<Reactor>>,
}

impl File {
    pub fn open(path: &str) -> Self {
        let path = std::ffi::CString::new(path).unwrap();
        // println!("path: {:?}", path.as_c_str().to_bytes());
        unsafe {
            let fd = libc::openat(AT_FDWCD as i32, path.as_ptr() as *const _, libc::O_RDONLY);
            let reactor = get_reactor();
            Self {
                fd,
                reactor: Rc::downgrade(&reactor),
            }
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
        let reactor = get_reactor();
        Self {
            fd,
            reactor: Rc::downgrade(&reactor),
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        unsafe {
            let reactor = self.reactor.upgrade().unwrap();
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

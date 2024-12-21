use std::{
    future::Future,
    os::fd::{AsRawFd, IntoRawFd, RawFd},
    task::{Context, Poll},
    io::{ErrorKind, Error as IoError},
};

use crate::reactor::get_reactor;

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

    pub fn read<'a>(&'a self, buf: &'a mut [u8]) -> FileRead<'a> {
        FileRead::new(self, buf)
    }

    pub fn write<'a>(&'a self, buf: &'a [u8]) -> FileWrite<'a> {
        FileWrite::new(self, buf)
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

pub struct FileRead<'a> {
    file: &'a File,
    buf: &'a mut [u8],
    token: Option<u64>,
}

impl<'a> FileRead<'a> {
    pub fn new(file: &'a File, buf: &'a mut [u8]) -> Self {
        Self {
            file,
            buf,
            token: None,
        }
    }
}

impl<'a> Future for FileRead<'a> {
    type Output = Result<usize, IoError>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let reactor = get_reactor();

        if self.token.is_none() {
            let token = reactor.borrow_mut().read(
                self.file.fd,
                cx,
                self.buf.as_mut_ptr() as *mut _,
                self.buf.len(),
            );

            self.token = Some(token);

            Poll::Pending
        } else {
            if let Some(result) = reactor.borrow_mut().take_token_result(self.token.unwrap()) {
                if result < 0 {
                    let err_code = -result;
                    match err_code {
                        libc::EAGAIN => Poll::Ready(Err(IoError::new(ErrorKind::WouldBlock, "Would block"))),
                        _ => Poll::Ready(Err(std::io::Error::from_raw_os_error(err_code))),
                    }
                } else {
                    Poll::Ready(Ok(result as usize))
                }
            } else {
                Poll::Pending
            }
        }
    }
}

pub struct FileWrite<'a> {
    file: &'a File,
    buf: &'a [u8],
    token: Option<u64>,
}

impl<'a> FileWrite<'a> {
    pub fn new(file: &'a File, buf: &'a [u8]) -> Self {
        Self {
            file,
            buf,
            token: None,
        }
    }
}

impl<'a> Future for FileWrite<'a> {
    type Output = Result<usize, std::io::Error>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let reactor = get_reactor();

        if self.token.is_none() {
            let token = reactor.borrow_mut().write(
                self.file.fd,
                cx,
                self.buf.as_ptr() as *const _,
                self.buf.len(),
            );
            self.token = Some(token);

            Poll::Pending
        } else {
            if let Some(result) = reactor.borrow_mut().take_token_result(self.token.unwrap()) {
                if result < 0 {
                    let err_code = -result;
                    match err_code {
                        libc::EAGAIN => Poll::Ready(Err(IoError::new(ErrorKind::WouldBlock, "Would block"))),
                        _ => Poll::Ready(Err(std::io::Error::from_raw_os_error(err_code))),
                    }
                } else {
                    Poll::Ready(Ok(result as usize))
                }
            } else {
                Poll::Pending
            }
        }
    }
}

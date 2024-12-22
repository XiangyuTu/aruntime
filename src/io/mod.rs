use std::{
    cell::RefCell,
    future::Future,
    io::{Result as IoResult, Error as IoError, ErrorKind},
    rc::{Rc, Weak},
    task::{Context, Poll},
};

use crate::reactor::{get_reactor, Reactor};

pub struct AsyncReader<'a> {
    fd: i32,
    buf: &'a mut [u8],
    token: Option<u64>,
    reactor: Weak<RefCell<Reactor>>,
}

impl<'a> AsyncReader<'a> {
    pub fn new(fd: i32, buf: &'a mut [u8]) -> Self {
        let reactor = get_reactor();
        Self {
            fd,
            buf,
            token: None,
            reactor: Rc::downgrade(&reactor),
        }
    }
}

impl<'a> Future for AsyncReader<'a> {
    type Output = IoResult<usize>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let reactor = self.reactor.upgrade().unwrap();

        if self.token.is_none() {
            let token = reactor.borrow_mut().read(
                self.fd,
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
                        libc::EAGAIN => {
                            Poll::Ready(Err(IoError::new(ErrorKind::WouldBlock, "Would block")))
                        }
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

pub struct AsyncWriter<'a> {
    fd: i32,
    buf: &'a [u8],
    token: Option<u64>,
    reactor: Weak<RefCell<Reactor>>,
}

impl<'a> AsyncWriter<'a> {
    pub fn new(fd: i32, buf: &'a [u8]) -> Self {
        let reactor = get_reactor();
        Self {
            fd,
            buf,
            token: None,
            reactor: Rc::downgrade(&reactor),
        }
    }
}

impl<'a> Future for AsyncWriter<'a> {
    type Output = IoResult<usize>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let reactor = self.reactor.upgrade().unwrap();

        if self.token.is_none() {
            let token = reactor.borrow_mut().write(
                self.fd,
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
                        libc::EAGAIN => {
                            Poll::Ready(Err(IoError::new(ErrorKind::WouldBlock, "Would block")))
                        }
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

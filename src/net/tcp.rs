use std::{
    cell::RefCell,
    future::Future,
    io::{Error as IoError, ErrorKind, Result as IoResult},
    net::{SocketAddr, ToSocketAddrs},
    os::fd::{AsRawFd, IntoRawFd, RawFd},
    rc::{Rc, Weak},
    task::{Context, Poll},
};

use futures::FutureExt;
use socket2::{Domain, Protocol, Socket, Type};

use crate::{
    io::{AsyncReader, AsyncWriter},
    reactor::{get_reactor, Reactor},
};

pub struct TcpListener {
    fd: RawFd,
    _reactor: Weak<RefCell<Reactor>>,
}

impl TcpListener {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> IoResult<Self> {
        let addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| IoError::new(ErrorKind::Other, "empty address"))?;

        let domain = if addr.is_ipv6() {
            Domain::IPV6
        } else {
            Domain::IPV4
        };
        let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
        let addr = socket2::SockAddr::from(addr);
        sock.set_reuse_address(true)?;
        sock.bind(&addr)?;
        sock.listen(1024)?;

        // add fd to reactor
        let reactor = get_reactor();

        println!("tcp bind with fd {}", sock.as_raw_fd());
        Ok(Self {
            fd: sock.into_raw_fd(),
            _reactor: Rc::downgrade(&reactor),
        })
    }

    pub fn accept(&self) -> TcpAccpeter {
        TcpAccpeter::new(self.fd)
    }
}

pub struct TcpAccpeter {
    fd: RawFd,
    reactor: Weak<RefCell<Reactor>>,
    token: Option<u64>,
}

impl TcpAccpeter {
    pub fn new(fd: RawFd) -> Self {
        let reactor = get_reactor();

        Self {
            fd,
            reactor: Rc::downgrade(&reactor),
            token: None,
        }
    }
}

impl Future for TcpAccpeter {
    type Output = IoResult<(TcpSteam, Option<SocketAddr>)>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let reactor = self.reactor.upgrade().unwrap();
        let mut socketaddr = Box::new((
            unsafe { std::mem::zeroed::<libc::sockaddr_storage>() },
            std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t,
        ));

        if self.token.is_none() {
            let token = reactor.borrow_mut().accept(
                self.fd,
                cx,
                &mut socketaddr.0 as *mut _ as *mut _,
                &mut socketaddr.1,
            );

            self.token = Some(token);

            Poll::Pending
        } else {
            let mut reactor = reactor.borrow_mut();
            if let Some(result) = reactor.take_token_result(self.token.unwrap()) {
                if result >= 0 {
                    let (_, addr) = unsafe {
                        socket2::SockAddr::init(move |addr_storage, len| {
                            socketaddr.0.clone_into(&mut *addr_storage);
                            *len = socketaddr.1;
                            Ok(())
                        })?
                    };

                    let stream = TcpSteam::new(result as RawFd);

                    Poll::Ready(Ok((stream, addr.as_socket())))
                } else {
                    let err_code = -result as i32;
                    let err = match err_code {
                        libc::EAGAIN => IoError::from(ErrorKind::WouldBlock),
                        _ => IoError::from(ErrorKind::Other),
                    };

                    Poll::Ready(Err(err))
                }
            } else {
                Poll::Pending
            }
        }
    }
}

pub struct TcpSteam {
    fd: RawFd,
    reactor: Weak<RefCell<Reactor>>,
}

impl TcpSteam {
    pub fn new(fd: RawFd) -> Self {
        let reactor = get_reactor();

        Self {
            fd,
            reactor: Rc::downgrade(&reactor),
        }
    }

    pub fn read<'a>(&'a self, buf: &'a mut [u8]) -> impl Future<Output = IoResult<usize>> + 'a {
        TcpStreamReader::new(self, buf)
    }

    pub fn write<'a>(&'a self, buf: &'a [u8]) -> impl Future<Output = IoResult<usize>> + 'a {
        TcpStreamWriter::new(self, buf)
    }
}

pub struct TcpStreamReader<'a> {
    reader: AsyncReader<'a>,
}

impl<'a> TcpStreamReader<'a> {
    pub fn new(stream: &'a TcpSteam, buf: &'a mut [u8]) -> Self {
        Self { reader: AsyncReader::new(stream.fd, buf) }
    }
}

impl<'a> Future for TcpStreamReader<'a> {
    type Output = IoResult<usize>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
       match self.reader.poll_unpin(cx) {
           Poll::Ready(Ok(n)) => {
                if n > 0 {
                    Poll::Ready(Ok(n))
                } else {
                    Poll::Ready(Err(IoError::from(ErrorKind::UnexpectedEof)))
                }
           }
           Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
           Poll::Pending => Poll::Pending,
       }
    }
}

impl Drop for TcpSteam {
    fn drop(&mut self) {
        unsafe {
            let reactor = self.reactor.upgrade().unwrap();
            reactor.borrow_mut().unregister_fd(self.fd);

            libc::close(self.fd);
        }
    }
}


pub struct TcpStreamWriter<'a> {
    writer: AsyncWriter<'a>,
}

impl<'a> TcpStreamWriter<'a> {
    pub fn new(stream: &'a TcpSteam, buf: &'a [u8]) -> Self {
        Self { writer: AsyncWriter::new(stream.fd, buf) }
    }
}

impl<'a> Future for TcpStreamWriter<'a> {
    type Output = IoResult<usize>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
       match self.writer.poll_unpin(cx) {
           Poll::Ready(Ok(n)) => Poll::Ready(Ok(n)),
           Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
           Poll::Pending => Poll::Pending,
       }
    }
}
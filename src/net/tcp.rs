use std::{
    cell::RefCell,
    future::Future,
    io,
    net::{ToSocketAddrs, SocketAddr},
    os::fd::{AsRawFd, RawFd},
    rc::{Rc, Weak},
    task::{Context, Poll},
};

use futures::future::Pending;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use crate::reactor::{self, get_reactor, Reactor};

pub struct TcpListener {
    fd: RawFd,
    _reactor: Weak<RefCell<Reactor>>,
}

impl TcpListener {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<Self, io::Error> {
        let addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "empty address"))?;

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
            fd: sock.as_raw_fd(),
            _reactor: Rc::downgrade(&reactor),
        })
    }

    pub fn accept(&self) -> AccpetFuture {
        AccpetFuture::new(self.fd)
    }
}

pub struct AccpetFuture {
    fd: RawFd,
    reactor: Weak<RefCell<Reactor>>,
    token: Option<u64>,
}

impl AccpetFuture {
    pub fn new(fd: RawFd) -> Self {
        let reactor = get_reactor();

        Self {
            fd,
            reactor: Rc::downgrade(&reactor),
            token: None,
        }
    }
}

impl Future for AccpetFuture {
    type Output = Result<Option<SocketAddr>, io::Error>;

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
                if result > 0 {
                    println!("accept success {}", result);

                    let (_, addr) = unsafe {
                        socket2::SockAddr::init(move |addr_storage, len| {
                            socketaddr.0.clone_into(&mut *addr_storage);
                            *len = socketaddr.1;
                            Ok(())
                        })?
                    };

                    Poll::Ready(Ok(addr.as_socket()))
                } else {
                    let token = reactor.accept(
                        self.fd,
                        cx,
                        &mut socketaddr.0 as *mut _ as *mut _,
                        &mut socketaddr.1,
                    );
                    self.token = Some(token);
                    
                    Poll::Pending
                }
            } else {
                Poll::Pending
            }
        }
    }
}

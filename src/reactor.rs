use std::{
    cell::RefCell,
    os::unix::prelude::{AsRawFd, RawFd},
    rc::Rc,
    task::{Context, Waker},
};

use io_uring::{opcode, types, IoUring};

#[inline]
pub(crate) fn get_reactor() -> Rc<RefCell<Reactor>> {
    crate::executor::EX.with(|ex| ex.reactor.clone())
}

pub struct Reactor {
    waker_mapping: rustc_hash::FxHashMap<u64, Vec<usize>>,
    wakers: Vec<Option<Waker>>,

    uring: IoUring,
    tokens_completion_result: Vec<Option<i32>>,
}

impl Reactor {
    pub fn new() -> Self {
        Self {
            waker_mapping: Default::default(),
            wakers: Vec::new(),

            uring: IoUring::new(128).unwrap(),
            tokens_completion_result: Vec::new(),
        }
    }
    
    fn register_waker(&mut self, fd: RawFd, waker: Waker) -> usize {
        // waker id
        let token = self.wakers.len();
        self.wakers.push(Some(waker));

        // register waker
        let waker_list = self.waker_mapping.entry(fd as u64).or_insert_with(Vec::new);
        waker_list.push(token);

        self.tokens_completion_result.push(None);

        token
    }

    fn unregister_wakers(&mut self, fd: RawFd, token: usize) {
        let waker_list = self.waker_mapping.get_mut(&(fd as u64)).unwrap();
        waker_list.retain(|&t| t != token);
    }
    
    pub fn fsync(&mut self, fd: impl AsRawFd, cx: &mut Context) -> usize {
        let token = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Fsync::new(types::Fd(fd.as_raw_fd())).build().user_data(token as u64);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    pub fn read(&mut self, fd: impl AsRawFd, cx: &mut Context, buf: *mut u8, len: usize) -> usize {
        let token = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Read::new(types::Fd(fd.as_raw_fd()), buf, len as u32).build().user_data(token as u64);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    pub fn readv(&mut self, fd: impl AsRawFd, cx: &mut Context, bufs: *const libc::iovec, bufs_len: usize)-> usize {
        let token: usize = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Readv::new(types::Fd(fd.as_raw_fd()), bufs, bufs_len as u32).build().user_data(token as u64);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    pub fn write(&mut self, fd: impl AsRawFd, cx: &mut Context, buf: *const u8, len: usize) -> usize {
        let token = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Write::new(types::Fd(fd.as_raw_fd()), buf, len as u32).build().user_data(token as u64);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    pub fn writev(&mut self, fd: impl AsRawFd, cx: &mut Context, bufs: *const libc::iovec, bufs_len: usize) -> usize {
        let token = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Writev::new(types::Fd(fd.as_raw_fd()), bufs, bufs_len as u32).build().user_data(token as u64);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    pub fn wait(&mut self) {
        let _ = self.uring.submit();
        for cqe in self.uring.completion() {
            let token = cqe.user_data();
            let result = cqe.result();
            println!("CQE : {:?}", token);
            let waker = self.wakers[token as usize].take().unwrap();
            assert!(self.wakers[token as usize].is_none());

            self.tokens_completion_result[token as usize] = Some(result);
            waker.wake();
            // remove waker
        }
    }

    pub(crate) fn is_token_completion(&self, token: usize) -> bool {
        self.tokens_completion_result[token].is_some()
    }

    pub fn get_token_result(&self, token: usize) -> Option<i32> {
        self.tokens_completion_result[token]
    }
}

impl Default for Reactor {
    fn default() -> Self {
        Self::new()
    }
}
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
    waker_mapping: rustc_hash::FxHashMap<u64, Vec<usize>>, // fd -> vec![waker_id], waker_id == token
    wakers: Vec<Option<(i32, Waker)>>, // waker_id -> (fd, waker)
    tokens_completion_result: Vec<Option<i32>>, // token -> result of completion

    uring: IoUring,

}

impl Reactor {
    pub fn new() -> Self {
        Self {
            waker_mapping: Default::default(),
            wakers: Vec::new(),
            tokens_completion_result: Vec::new(),

            uring: IoUring::new(128).unwrap(),
        }
    }
    
    fn register_waker(&mut self, fd: RawFd, waker: Waker) -> u64 {
        // waker id
        let token = self.wakers.len();
        self.wakers.push(Some((fd, waker)));

        // register waker
        let waker_list = self.waker_mapping.entry(fd as u64).or_insert_with(Vec::new);
        waker_list.push(token);

        self.tokens_completion_result.push(None);

        token as u64
    }

    pub(crate) fn unregister_fd(&mut self, fd: RawFd) {
        if let Some(tokens) = self.waker_mapping.get(&(fd as u64)) {
            for token in tokens {
                self.wakers[*token] = None;
            }

            self.waker_mapping.remove(&(fd as u64));
        }
    }
    
    #[allow(dead_code)]
    pub(crate) fn fsync(&mut self, fd: impl AsRawFd, cx: &mut Context) -> u64 {
        let token = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Fsync::new(types::Fd(fd.as_raw_fd())).build().user_data(token);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    pub(crate) fn read(&mut self, fd: impl AsRawFd, cx: &mut Context, buf: *mut u8, len: usize) -> u64 {
        let token = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Read::new(types::Fd(fd.as_raw_fd()), buf, len as u32).build().user_data(token);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    #[allow(dead_code)]
    pub(crate) fn readv(&mut self, fd: impl AsRawFd, cx: &mut Context, bufs: *const libc::iovec, bufs_len: usize)-> u64 {
        let token = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Readv::new(types::Fd(fd.as_raw_fd()), bufs, bufs_len as u32).build().user_data(token);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    pub(crate) fn write(&mut self, fd: impl AsRawFd, cx: &mut Context, buf: *const u8, len: usize) -> u64 {
        let token = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Write::new(types::Fd(fd.as_raw_fd()), buf, len as u32).build().user_data(token);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    #[allow(dead_code)]
    pub(crate) fn writev(&mut self, fd: impl AsRawFd, cx: &mut Context, bufs: *const libc::iovec, bufs_len: usize) -> u64 {
        let token = self.register_waker(fd.as_raw_fd(), cx.waker().clone());

        let sqe = opcode::Writev::new(types::Fd(fd.as_raw_fd()), bufs, bufs_len as u32).build().user_data(token);
        unsafe { self.uring.submission().push(&sqe).unwrap() };

        token
    }

    pub fn wait(&mut self) {
        let _ = self.uring.submit();
        for cqe in self.uring.completion() {
            let token = cqe.user_data();
            let result = cqe.result();

            println!("CQE token: {:?}", token);
            let (fd, waker) = self.wakers[token as usize].take().unwrap();
            // waker had been removed from wakers
            assert!(self.wakers[token as usize].is_none());
            // remove this walker from waker_mapping
            self.waker_mapping.get_mut(&(fd as u64)).unwrap().retain(|&t| t != token as usize);
            // set result
            self.tokens_completion_result[token as usize] = Some(result);
            waker.wake();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn is_token_completion(&self, token: u64) -> bool {
        self.tokens_completion_result[token as usize].is_some()
    }

    pub(crate) fn take_token_result(&mut self, token: u64) -> Option<i32> {
        // take result
        self.tokens_completion_result[token as usize].take()
    }
}

impl Default for Reactor {
    fn default() -> Self {
        Self::new()
    }
}
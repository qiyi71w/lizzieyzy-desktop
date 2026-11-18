use super::{encode, Framer};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};

static CONNECTION_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const READ_ROUNDS: usize = 16;
const ACCEPT_ROUNDS: usize = 8;
const WRITE_ROUNDS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportEvent {
    Connected { token: u64 },
    Received { token: u64, body: String },
    Disconnected { token: u64 },
    Problem { token: Option<u64>, key: String },
}
struct PendingWrite { frame: Vec<u8>, offset: usize }
struct Peer {
    stream: TcpStream,
    token: u64,
    framer: Framer,
    pending: Option<PendingWrite>,
}

/// A single loopback peer. All calls are serialized by the workbench's sole writer.
/// `poll` never blocks: at most eight accepts, sixteen 4 KiB reads and sixteen
/// writes. One bounded frame may be in flight; no queue survives a peer token.
/// A claimed write cannot be retracted; only an official echo confirms it.
pub struct Listener {
    server: Option<TcpListener>,
    port: u16,
    peer: Option<Peer>,
    deferred: Vec<TransportEvent>,
}
impl Listener {
    pub fn bind(port: u16) -> io::Result<Self> {
        let server = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        server.set_nonblocking(true)?;
        let port = server.local_addr()?.port();
        Ok(Self { server: Some(server), port, peer: None, deferred: Vec::new() })
    }
    pub fn local_port(&self) -> u16 { self.port }

    /// `false` means no permission was consumed (wrong peer, busy/unwritable, or
    /// rejected claim). `true` means write-start was claimed exactly once; a
    /// partial remainder is flushed by `poll`, never resent or claimed again.
    /// Later I/O failures are delivered as token-bound Problem + Disconnected.
    pub fn send(&mut self, token: u64, body: &str, claim: impl FnOnce() -> bool) -> io::Result<bool> {
        let Some(peer) = &mut self.peer else { return Ok(false); };
        if peer.token != token || peer.pending.is_some() { return Ok(false); }
        let ready = writable(&peer.stream).and_then(|ready| {
            if !ready { return Ok(false); }
            // POLLOUT alone may also describe a half-closed socket. Do not claim
            // a new submission after EOF already observable on this peer.
            let mut byte = [0];
            match peer.stream.peek(&mut byte) {
                Ok(0) => Err(io::Error::new(io::ErrorKind::NotConnected, "Fox peer disconnected")),
                Ok(_) => Ok(true),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(true),
                Err(e) => Err(e),
            }
        });
        match ready {
            Ok(false) => return Ok(false),
            Err(error) => { self.retire(Some("Fox.connectionError")); return Err(error); }
            Ok(true) => {}
        }
        let frame = encode(body).map_err(|s| io::Error::new(io::ErrorKind::InvalidInput, s))?;
        if !claim() { return Ok(false); }
        peer.pending = Some(PendingWrite { frame, offset: 0 });
        if let Err(error) = flush(peer) {
            self.retire(Some("Fox.connectionError"));
            return Err(error);
        }
        Ok(true)
    }

    pub fn poll(&mut self) -> Vec<TransportEvent> {
        // A failed send must deliver old disconnection before replacement admission.
        if !self.deferred.is_empty() { return std::mem::take(&mut self.deferred); }
        if self.server.is_none() { return Vec::new(); }
        let mut events = Vec::new();
        if let Some(peer) = &mut self.peer {
            let mut problem = None;
            let mut disconnected = false;
            let mut buffer = [0; 4096];
            for _ in 0..READ_ROUNDS {
                match peer.stream.read(&mut buffer) {
                    Ok(0) => { disconnected = true; break; }
                    Ok(n) => {
                        let token = peer.token;
                        if peer.framer.push(&buffer[..n], |body| events.push(TransportEvent::Received { token, body })).is_err() {
                            problem = Some("Fox.protocolError");
                            disconnected = true;
                            break;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => { problem = Some("Fox.connectionError"); disconnected = true; break; }
                }
            }
            if !disconnected && flush(peer).is_err() { problem = Some("Fox.connectionError"); disconnected = true; }
            if disconnected {
                self.retire(problem);
                events.append(&mut self.deferred);
                // The caller consumes the old transition before accepting the successor.
                return events;
            }
        }
        for _ in 0..ACCEPT_ROUNDS {
            let accepted = self.server.as_ref().expect("open server").accept();
            match accepted {
                Ok((stream, _)) => {
                    if self.peer.is_some() {
                        let _ = stream.shutdown(Shutdown::Both);
                        events.push(TransportEvent::Problem { token: None, key: "Fox.duplicateClient".into() });
                        continue;
                    }
                    if stream.set_nonblocking(true).and_then(|()| stream.set_nodelay(true)).is_err() {
                        let _ = stream.shutdown(Shutdown::Both);
                        events.push(TransportEvent::Problem { token: None, key: "Fox.connectionError".into() });
                        continue;
                    }
                    let token = CONNECTION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
                    self.peer = Some(Peer { stream, token, framer: Framer::default(), pending: None });
                    events.push(TransportEvent::Connected { token });
                    // Deliver connection before any of its received frames.
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    events.push(TransportEvent::Problem { token: None, key: "Fox.connectionError".into() });
                    break;
                }
            }
        }
        events
    }
    fn retire(&mut self, problem: Option<&str>) {
        if let Some(peer) = self.peer.take() {
            let _ = peer.stream.shutdown(Shutdown::Both);
            if let Some(key) = problem {
                self.deferred.push(TransportEvent::Problem { token: Some(peer.token), key: key.into() });
            }
            self.deferred.push(TransportEvent::Disconnected { token: peer.token });
        }
    }
    pub fn close(&mut self) {
        self.server = None;
        if let Some(peer) = self.peer.take() { let _ = peer.stream.shutdown(Shutdown::Both); }
        self.deferred.clear();
    }
}
impl Drop for Listener { fn drop(&mut self) { self.close(); } }

fn flush(peer: &mut Peer) -> io::Result<()> {
    let Some(pending) = &mut peer.pending else { return Ok(()); };
    for _ in 0..WRITE_ROUNDS {
        match peer.stream.write(&pending.frame[pending.offset..]) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "Fox write returned zero")),
            Ok(n) => {
                pending.offset += n;
                if pending.offset == pending.frame.len() { peer.pending = None; return Ok(()); }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

// std exposes nonblocking sockets but no zero-time writable readiness check.
// These tiny OS adapters avoid a reactor/thread and keep claim after readiness.
#[cfg(unix)]
fn writable(stream: &TcpStream) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    #[repr(C)]
    struct PollFd { fd: std::ffi::c_int, events: i16, revents: i16 }
    #[cfg(target_os = "macos")]
    type Nfds = u32;
    #[cfg(not(target_os = "macos"))]
    type Nfds = usize;
    unsafe extern "C" { fn poll(fds: *mut PollFd, count: Nfds, timeout: std::ffi::c_int) -> std::ffi::c_int; }
    let mut fd = PollFd { fd: stream.as_raw_fd(), events: 4, revents: 0 };
    // SAFETY: fd points to one initialized pollfd for this call; socket remains owned.
    let result = unsafe { poll(&mut fd, 1, 0) };
    if result < 0 { return Err(io::Error::last_os_error()); }
    if fd.revents & (8 | 16 | 32) != 0 { return Err(io::Error::new(io::ErrorKind::NotConnected, "Fox socket unavailable")); }
    Ok(fd.revents & 4 != 0)
}
#[cfg(windows)]
fn writable(stream: &TcpStream) -> io::Result<bool> {
    use std::os::windows::io::AsRawSocket;
    #[repr(C)]
    struct PollFd { fd: usize, events: i16, revents: i16 }
    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn WSAPoll(fds: *mut PollFd, count: u32, timeout: i32) -> i32;
        fn WSAGetLastError() -> i32;
    }
    let mut fd = PollFd { fd: stream.as_raw_socket() as usize, events: 0x10, revents: 0 };
    // SAFETY: one initialized WSAPOLLFD, valid owned Winsock socket, zero timeout.
    let result = unsafe { WSAPoll(&mut fd, 1, 0) };
    if result < 0 { return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() })); }
    if fd.revents & 7 != 0 { return Err(io::Error::new(io::ErrorKind::NotConnected, "Fox socket unavailable")); }
    Ok(fd.revents & 0x10 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fox::{GameState, SubmissionOutcome};
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    fn until(listener: &mut Listener, predicate: impl Fn(&[TransportEvent]) -> bool) -> Vec<TransportEvent> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut all = Vec::new();
        loop {
            all.extend(listener.poll());
            if predicate(&all) { return all; }
            assert!(Instant::now() < deadline, "transport condition timed out: {all:?}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn connected() -> (Listener, TcpStream, u64) {
        let mut listener = Listener::bind(0).unwrap();
        let peer = TcpStream::connect((Ipv4Addr::LOCALHOST, listener.local_port())).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let events = until(&mut listener, |events| events.iter().any(|e| matches!(e, TransportEvent::Connected { .. })));
        let token = events.iter().find_map(|e| match e { TransportEvent::Connected { token } => Some(*token), _ => None }).unwrap();
        (listener, peer, token)
    }
    fn ready_game() -> GameState {
        let mut game = GameState::default();
        game.connected(0);
        for message in ["FARULE,19,60,30,3,650", "FASTATUS,1,B", "FATIMELEFT,60,30,3", "FAMOVE,1,B,0^0^0^N"] {
            game.receive(message, 0).unwrap();
        }
        assert!(game.begin_clock_refresh(0));
        game.receive("FATIMELEFT,60,30,3", 0).unwrap();
        game
    }

    #[test]
    fn loopback_frames_are_fragmented_coalesced_and_token_bound() {
        let (mut listener, mut peer, token) = connected();
        peer.write_all(b"$FASTATUS,").unwrap();
        assert!(listener.poll().iter().all(|e| !matches!(e, TransportEvent::Received { .. })));
        peer.write_all(b"0*0F\r\n$FASTATUS,0*0F\r\n").unwrap();
        let events = until(&mut listener, |events| events.iter().filter(|e| matches!(e, TransportEvent::Received { .. })).count() == 2);
        assert_eq!(events, [TransportEvent::Received { token, body: "FASTATUS,0".into() }, TransportEvent::Received { token, body: "FASTATUS,0".into() }]);
        assert!(listener.send(token, "AFSTATUS,B", || true).unwrap());
        let expected = encode("AFSTATUS,B").unwrap();
        let mut bytes = vec![0; expected.len()];
        peer.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn permission_is_never_consumed_for_stale_token_cancelled_or_expired_work() {
        let (mut listener, mut peer, token) = connected();
        let called = Cell::new(false);
        assert!(!listener.send(token + 1, "AFSKIP,B", || { called.set(true); true }).unwrap());
        assert!(!called.get());
        for expired in [false, true] {
            let mut game = ready_game();
            game.set_direct_max_time(2.0).unwrap();
            let pending = game.prepare_submission(&game.snapshot(), 1, "D4", 1).unwrap().unwrap();
            if !expired { assert!(game.cancel_unwritten_for_pda(2)); }
            assert!(!listener.send(token, &pending.body, || game.claim_submission(&pending, 3_000_000_000)).unwrap());
            assert_eq!(game.latest_submission().unwrap().outcome, if expired { SubmissionOutcome::Pending } else { SubmissionOutcome::Cancelled });
        }
        peer.set_nonblocking(true).unwrap();
        let mut byte = [0];
        assert_eq!(peer.read(&mut byte).unwrap_err().kind(), io::ErrorKind::WouldBlock);
    }

    #[test]
    fn claimed_move_writes_once_and_only_official_echo_confirms_it() {
        let (mut listener, mut peer, token) = connected();
        let mut game = ready_game();
        let pending = game.prepare_submission(&game.snapshot(), 1, "D4", 1).unwrap().unwrap();
        assert!(listener.send(token, &pending.body, || game.claim_submission(&pending, 2)).unwrap());
        assert!(!listener.send(token, &pending.body, || game.claim_submission(&pending, 3)).unwrap());
        assert_eq!(game.latest_submission().unwrap().outcome, SubmissionOutcome::Pending);
        assert!(game.snapshot().actions.is_empty());
        let expected = encode(&pending.body).unwrap();
        let mut bytes = vec![0; expected.len()];
        peer.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
        peer.write_all(&encode("FAMOVE,0,B,1^3^15^B").unwrap()).unwrap();
        let events = until(&mut listener, |events| events.iter().any(|e| matches!(e, TransportEvent::Received { .. })));
        for event in events {
            if let TransportEvent::Received { token: received, body } = event {
                assert_eq!(received, token);
                game.receive(&body, 4).unwrap();
            }
        }
        assert_eq!(game.latest_submission().unwrap().outcome, SubmissionOutcome::Confirmed);
        assert_eq!(game.snapshot().actions.len(), 1);
    }

    #[test]
    fn old_disconnect_is_delivered_before_replacement_and_old_permission_cannot_cross_token() {
        let (mut listener, peer, old) = connected();
        let mut events = Vec::new();
        peer.shutdown(Shutdown::Both).unwrap();
        let replacement = TcpStream::connect((Ipv4Addr::LOCALHOST, listener.local_port())).unwrap();
        // Whether the replacement is refused depends on whether the old peer's EOF was seen before
        // the replacement was accepted (Java `FoxConnection` also refuses a client while the old
        // slot is held). Either way the old disconnection is delivered on its own, first; the
        // bound only stops a broken run from hanging.
        let guard = Instant::now() + Duration::from_secs(30);
        loop {
            let polled = listener.poll();
            assert!(!polled.iter().any(|e| matches!(e, TransportEvent::Connected { .. })), "admitted before the old disconnection: {events:?} then {polled:?}");
            let done = polled.contains(&TransportEvent::Disconnected { token: old });
            if done {
                assert_eq!(polled, [TransportEvent::Disconnected { token: old }], "after {events:?}");
            }
            events.extend(polled);
            if done { break; }
            assert!(Instant::now() < guard, "waiting for the old disconnection; events: {events:?}");
            std::thread::sleep(Duration::from_millis(1));
        }
        let duplicate = TransportEvent::Problem { token: None, key: "Fox.duplicateClient".into() };
        assert!(events.iter().all(|e| *e == duplicate || *e == TransportEvent::Disconnected { token: old }), "{events:?}");
        // A refused replacement was closed; the client connects again.
        let mut replacement = if events.contains(&duplicate) {
            TcpStream::connect((Ipv4Addr::LOCALHOST, listener.local_port())).unwrap()
        } else {
            replacement
        };
        replacement.set_nonblocking(true).unwrap();
        let mut admitted = Vec::new();
        let next = loop {
            admitted.extend(listener.poll());
            if let Some(next) = admitted.iter().find_map(|e| match e { TransportEvent::Connected { token } => Some(*token), _ => None }) {
                break next;
            }
            assert!(Instant::now() < guard, "waiting for the replacement; events: {events:?} then {admitted:?}");
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(admitted, [TransportEvent::Connected { token: next }]);
        assert_ne!(old, next);
        let called = Cell::new(false);
        assert!(!listener.send(old, "AFSKIP,B", || { called.set(true); true }).unwrap());
        assert!(!called.get());
        let mut byte = [0];
        assert_eq!(replacement.read(&mut byte).unwrap_err().kind(), io::ErrorKind::WouldBlock);
    }

    #[test]
    fn invalid_frame_disconnects_peer_with_protocol_problem_and_listener_remains_usable() {
        let (mut listener, mut peer, token) = connected();
        peer.write_all(b"$FASTATUS,0*00\r\n").unwrap();
        let events = until(&mut listener, |events| events.iter().any(|e| matches!(e, TransportEvent::Disconnected { .. })));
        assert_eq!(events, [TransportEvent::Problem { token: Some(token), key: "Fox.protocolError".into() }, TransportEvent::Disconnected { token }]);
        let _replacement = TcpStream::connect((Ipv4Addr::LOCALHOST, listener.local_port())).unwrap();
        until(&mut listener, |events| events.iter().any(|e| matches!(e, TransportEvent::Connected { .. })));
        listener.close();
        assert!(!listener.send(token, "AFSKIP,B", || panic!("closed listener must not claim")).unwrap());
        assert!(listener.poll().is_empty());
    }

    #[test]
    fn validated_confirmation_precedes_protocol_problem_and_disconnect() {
        let (mut listener, mut peer, token) = connected();
        let confirmation = encode("FAMOVE,0,B,1^3^15^B").unwrap();
        let later = encode("FASTATUS,0").unwrap();
        peer.write_all(&[confirmation.as_slice(), b"$BAD*00\r\n", later.as_slice()].concat()).unwrap();
        let events = until(&mut listener, |events| events.iter().any(|e| matches!(e, TransportEvent::Disconnected { .. })));
        assert_eq!(events, [
            TransportEvent::Received { token, body: "FAMOVE,0,B,1^3^15^B".into() },
            TransportEvent::Problem { token: Some(token), key: "Fox.protocolError".into() },
            TransportEvent::Disconnected { token },
        ]);
        assert!(listener.poll().is_empty());
    }

    #[test]
    fn duplicate_peer_is_rejected_without_replacing_authoritative_peer() {
        let (mut listener, mut peer, token) = connected();
        let _duplicate = TcpStream::connect((Ipv4Addr::LOCALHOST, listener.local_port())).unwrap();
        let events = until(&mut listener, |events| events.iter().any(|e| matches!(e, TransportEvent::Problem { .. })));
        assert_eq!(events, [TransportEvent::Problem { token: None, key: "Fox.duplicateClient".into() }]);
        assert!(listener.send(token, "AFSKIP,B", || true).unwrap());
        let expected = encode("AFSKIP,B").unwrap();
        let mut actual = vec![0; expected.len()];
        peer.read_exact(&mut actual).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn backpressure_is_bounded_and_accepted_partial_frames_are_never_reclaimed_or_duplicated() {
        let (mut listener, mut peer, token) = connected();
        peer.set_nonblocking(true).unwrap();
        let body = "X".repeat(65_530);
        let frame = encode(&body).unwrap();
        let claims = Cell::new(0);
        let mut accepted = 0;
        let mut backpressured = false;
        for _ in 0..512 {
            if listener.send(token, &body, || { claims.set(claims.get() + 1); true }).unwrap() {
                accepted += 1;
            } else { backpressured = true; break; }
        }
        assert!(backpressured, "non-reading peer must eventually exert backpressure");
        assert_eq!(claims.get(), accepted);
        let expected = frame.repeat(accepted);
        let mut actual = Vec::new();
        let mut buffer = [0; 65_536];
        let deadline = Instant::now() + Duration::from_secs(5);
        while actual.len() < expected.len() {
            assert!(listener.poll().is_empty());
            match peer.read(&mut buffer) {
                Ok(0) => panic!("unexpected EOF"),
                Ok(n) => actual.extend_from_slice(&buffer[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(1)),
                Err(e) => panic!("read failed: {e}"),
            }
            assert!(Instant::now() < deadline, "partial frame did not drain");
        }
        assert_eq!(actual, expected);
        assert_eq!(claims.get(), accepted, "poll must not re-claim partial writes");
        assert!(listener.send(token, "AFSTATUS,B", || true).unwrap());
        let mut extra = vec![0; encode("AFSTATUS,B").unwrap().len()];
        peer.set_nonblocking(false).unwrap();
        peer.read_exact(&mut extra).unwrap();
        assert_eq!(extra, encode("AFSTATUS,B").unwrap());
    }
}

use std::io::Seek;

use rustix::event::{PollFd, PollFlags, Timespec};
use tinywasm::FuncContext;

use crate::p1::abi::*;
use crate::p1::ctx::{Resource, state};
use crate::p1::memory::GuestMemory;

use super::WasiResult;

enum Readiness {
    Ready,
    Clock(u64),
    Poll,
}

struct PendingEvent {
    bytes: [u8; 32],
    readiness: Readiness,
}

struct PollTarget {
    event_index: usize,
    fd: i32,
    event_type: u8,
}

impl PendingEvent {
    fn new(userdata: u64, error: Errno, event_type: u8, available: u64, readiness: Readiness) -> Self {
        let mut bytes = [0; 32];
        put(&mut bytes, 0, userdata.to_le_bytes());
        put(&mut bytes, 8, (i32::from(error) as u16).to_le_bytes());
        bytes[10] = event_type;
        put(&mut bytes, 16, available.to_le_bytes());
        Self { bytes, readiness }
    }
}

pub(super) fn poll_oneoff(
    mut ctx: FuncContext<'_>,
    (subscriptions_ptr, events_ptr, count, result_ptr): (i32, i32, i32, i32),
) -> WasiResult<Errno> {
    let count = count as u32 as usize;
    if count == 0 || count > 1024 {
        return Ok(INVAL);
    }
    let memory = GuestMemory::new(&ctx)?;
    let subscriptions = memory.read_at(&ctx, subscriptions_ptr as u32 as usize, count * 48)?;
    memory.check_range(&ctx, events_ptr as u32 as usize, count * 32)?;
    memory.check_range(&ctx, result_ptr as u32 as usize, 4)?;

    let mut pending_events = Vec::with_capacity(count);
    let mut poll_targets = Vec::new();
    let mut minimum_wait: Option<u64> = None;
    for subscription in subscriptions.as_chunks::<48>().0 {
        let userdata = u64::from_le_bytes(subscription[..8].try_into().expect("userdata"));
        let event_type = subscription[8];
        if event_type == 0 {
            let wait = clock_wait(&ctx, subscription)?;
            minimum_wait = Some(minimum_wait.map_or(wait, |current| current.min(wait)));
            pending_events.push(PendingEvent::new(userdata, SUCCESS, event_type, 0, Readiness::Clock(wait)));
        } else if matches!(event_type, 1 | 2) {
            let fd = u32::from_le_bytes(subscription[16..20].try_into().expect("fd")) as i32;
            subscribe_fd(&ctx, fd, userdata, event_type, &mut pending_events, &mut poll_targets)?;
        } else {
            return Ok(INVAL);
        }
    }

    let immediately_ready =
        pending_events.iter().any(|event| matches!(event.readiness, Readiness::Ready | Readiness::Clock(0)));
    let timeout = if immediately_ready { Some(0) } else { minimum_wait };
    let elapsed = wait_for_events(&ctx, &poll_targets, &mut pending_events, timeout)?;
    let mut bytes = Vec::with_capacity(pending_events.len() * 32);
    for event in pending_events {
        if matches!(event.readiness, Readiness::Ready)
            || matches!(event.readiness, Readiness::Clock(wait) if wait <= elapsed)
        {
            bytes.extend_from_slice(&event.bytes);
        }
    }
    let event_count = (bytes.len() / 32) as u32;
    memory.write(ctx.store_mut(), events_ptr, &bytes)?;
    Ok(memory.write(ctx.store_mut(), result_ptr, &event_count.to_le_bytes()).map_or_else(|errno| errno, |()| SUCCESS))
}

fn clock_wait(ctx: &FuncContext<'_>, subscription: &[u8; 48]) -> WasiResult<u64> {
    let clock_id = u32::from_le_bytes(subscription[16..20].try_into().expect("clock id"));
    let timeout = u64::from_le_bytes(subscription[24..32].try_into().expect("timeout"));
    let flags = u16::from_le_bytes(subscription[40..42].try_into().expect("flags"));
    if !matches!(clock_id, 0 | 1) || flags & !1 != 0 {
        return Err(INVAL.into());
    }
    let now = if clock_id == 0 {
        realtime_now()
    } else {
        state(ctx)?.monotonic_start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX)
    };
    Ok(if flags & 1 != 0 { timeout.saturating_sub(now) } else { timeout })
}

fn subscribe_fd(
    ctx: &FuncContext<'_>,
    fd: i32,
    userdata: u64,
    event_type: u8,
    pending_events: &mut Vec<PendingEvent>,
    poll_targets: &mut Vec<PollTarget>,
) -> WasiResult<()> {
    let required_right = if event_type == 1 { RIGHT_FD_READ } else { RIGHT_FD_WRITE };
    let descriptor = match state(ctx)?.descriptor(fd) {
        Ok(descriptor) => descriptor,
        Err(errno) => {
            pending_events.push(PendingEvent::new(userdata, errno, event_type, 0, Readiness::Ready));
            return Ok(());
        }
    };
    if let Err(errno) = descriptor.require_rights(required_right | RIGHT_POLL_FD_READWRITE) {
        pending_events.push(PendingEvent::new(userdata, errno, event_type, 0, Readiness::Ready));
        return Ok(());
    }
    if let Resource::File(file) = &descriptor.resource {
        let available = if event_type == 1 {
            file.try_clone()
                .and_then(|mut file| {
                    let position = file.stream_position()?;
                    Ok(file.metadata()?.len().saturating_sub(position))
                })
                .unwrap_or(0)
        } else {
            0
        };
        pending_events.push(PendingEvent::new(userdata, SUCCESS, event_type, available, Readiness::Ready));
        return Ok(());
    }
    poll_targets.push(PollTarget { event_index: pending_events.len(), fd, event_type });
    pending_events.push(PendingEvent::new(userdata, SUCCESS, event_type, 0, Readiness::Poll));
    Ok(())
}

fn wait_for_events(
    ctx: &FuncContext<'_>,
    poll_targets: &[PollTarget],
    pending_events: &mut [PendingEvent],
    timeout_ns: Option<u64>,
) -> WasiResult<u64> {
    let started = std::time::Instant::now();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let wasi = state(ctx)?;
    let mut poll_fds = Vec::with_capacity(poll_targets.len());
    for target in poll_targets {
        let flags = if target.event_type == 1 { PollFlags::IN } else { PollFlags::OUT };
        let descriptor = wasi.descriptor(target.fd)?;
        let poll_fd = match &descriptor.resource {
            Resource::Stdin => PollFd::new(&stdin, flags),
            Resource::Stdout => PollFd::new(&stdout, flags),
            Resource::Stderr => PollFd::new(&stderr, flags),
            Resource::TcpListener(listener) => PollFd::new(listener, flags),
            Resource::TcpStream(stream) => PollFd::new(stream, flags),
            Resource::UdpSocket(socket) => PollFd::new(socket, flags),
            Resource::File(_) | Resource::Directory(_) => unreachable!(),
        };
        poll_fds.push(poll_fd);
    }
    let timeout = timeout_ns.map(|nanoseconds| Timespec {
        tv_sec: (nanoseconds / 1_000_000_000).try_into().unwrap_or(i64::MAX),
        tv_nsec: (nanoseconds % 1_000_000_000) as _,
    });
    if poll_fds.is_empty() {
        if let Some(nanoseconds) = timeout_ns {
            std::thread::sleep(std::time::Duration::from_nanos(nanoseconds));
        }
    } else {
        loop {
            match rustix::event::poll(&mut poll_fds, timeout.as_ref()) {
                Err(rustix::io::Errno::INTR) => continue,
                result => {
                    result.map_err(Errno::from)?;
                    break;
                }
            }
        }
    }
    for (target, poll_fd) in poll_targets.iter().zip(poll_fds) {
        let flags = poll_fd.revents();
        if flags.intersects(PollFlags::IN | PollFlags::OUT | PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL) {
            pending_events[target.event_index].readiness = Readiness::Ready;
            let error = if flags.contains(PollFlags::NVAL) {
                BADF
            } else if flags.contains(PollFlags::ERR) {
                IO
            } else {
                SUCCESS
            };
            put(&mut pending_events[target.event_index].bytes, 8, (i32::from(error) as u16).to_le_bytes());
            if target.event_type == 1 {
                let resource = &wasi.descriptor(target.fd)?.resource;
                let available = match resource {
                    Resource::Stdin => rustix::io::ioctl_fionread(&stdin),
                    Resource::TcpStream(stream) => rustix::io::ioctl_fionread(stream),
                    Resource::UdpSocket(socket) => rustix::io::ioctl_fionread(socket),
                    _ => Ok(0),
                }
                .unwrap_or(0);
                put(&mut pending_events[target.event_index].bytes, 16, available.to_le_bytes());
                if matches!(resource, Resource::TcpStream(_)) && available == 0 {
                    put(&mut pending_events[target.event_index].bytes, 24, 1_u16.to_le_bytes());
                }
            }
            if flags.contains(PollFlags::HUP) {
                put(&mut pending_events[target.event_index].bytes, 24, 1_u16.to_le_bytes());
            }
        }
    }
    Ok(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX))
}

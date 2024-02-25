use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpStream;

enum Status {
    // continue
    Continue,
    // regular exit
    ExitOk
}

fn send_exit_pkt(
    stream: &mut impl std::io::Write,
    exit_reason: u32
) {
    // build packet
    // exit packet: type 2
    stream.write(&(2 as u32).to_be_bytes()).unwrap();
    // exit reason, only 0 in currently valid
    stream.write(&exit_reason.to_be_bytes()).unwrap();
    // send packet
    stream.flush().unwrap();
}

fn handle_local2remote_pkt(
    iffile: &mut std::fs::File,
    stream: &mut impl std::io::Write,
    counter: &mut u64,
    buffer: &mut [u8],
) -> Status {
    // packet is always fully read (if possible):
    // this is a special case tied to virtual interface
    // internals
    let sz = match iffile.read(buffer) {
        Ok(0) => {
            panic!("UNEXPECTED EMPTY PACKET!");
        }
        Ok(sz) => {
            // new packet
            *counter += 1;
            sz
        }
        Err(err) => {
            eprintln!("Error creating cstring: {}", err);
            std::process::exit(1)
        }
    };
    // build packet
    // data packet: type 1
    stream.write(&(1 as u32).to_be_bytes()).unwrap();
    // pkt length
    stream.write(&(sz as u32).to_be_bytes()).unwrap();
    // counter
    stream.write(&counter.to_be_bytes()).unwrap();
    // network packet
    stream.write(&buffer[..sz]).unwrap();
    // send packet
    stream.flush().unwrap();
    // Everything Ok, continue
    Status::Continue
}

fn handle_remote2local_pkt(
    iffile: &mut std::fs::File,
    stream: &mut impl std::io::BufRead,
    buffer: &mut [u8],
) -> Status {
    // read packet type
    let mut pkt_type: [u8; 4] = [0; 4];
    stream.read_exact(&mut pkt_type).unwrap();
    let pkt_type = u32::from_be_bytes(pkt_type);
    match pkt_type {
        1 => {
            let mut pkt_len: [u8; 4] = [0; 4];
            stream.read_exact(&mut pkt_len).unwrap();
            let pkt_len: u32 = u32::from_be_bytes(pkt_len);
            //println!("pkt_len = {}", pkt_len);
            let mut counter: [u8; 8] = [0; 8];
            stream.read_exact(&mut counter).unwrap();
            let _counter = u64::from_be_bytes(counter);
            // counter is unused now
            stream
                .read_exact(&mut buffer[0..(pkt_len as usize)])
                .unwrap();
            // https://doc.rust-lang.org/std/fs/struct.File.html#method.write_all_at-1
            match iffile.write_all(&buffer[0..(pkt_len as usize)]) {
                Ok(()) => (),
                Err(_) => eprintln!("Error writing pkt to virtual interface"),
            };
            // it does not seem possible to flush virtual interface fd
            //iffile.flush().unwrap();
        }
        2 => {
            let mut exit_reason: [u8; 4] = [0; 4];
            stream.read_exact(&mut exit_reason).unwrap();
            let exit_reason: u32 = u32::from_be_bytes(exit_reason);
            if exit_reason != 0 {
                panic!("Unknown exit reason code {} in VPN protocol", exit_reason);
            }
            // terminate VPN protocol
            return Status::ExitOk;
        }
        _ => {
            panic!("Unknown packet type: {} (only 1 valid)", pkt_type);
        }
    }
    // Everything Ok, continue
    Status::Continue
}

// https://docs.rs/nix/0.28.0/nix/poll/struct.PollFd.html
// sigfile has been generated by crate::signals::spawn_sig_handler
// and is filled with new data everytime a signal is received
//
// Return true if exits because received exit packet from remote
// endpoint (or in case of remote stream error), return false if
// it exits because of local signal
//
// abort in case of other errors
pub fn handle_flow(
    stream: &mut TcpStream,
    iffile: &mut std::fs::File,
    sigfile: &mut std::fs::File
) -> bool {
    // buffer
    let mut buffer: [u8; 4096] = [0; 4096];
    // split both socket ends
    let mut ostream = BufWriter::with_capacity(64 + 4096, stream.try_clone().unwrap());
    let mut istream = BufReader::with_capacity(64 + 4096, stream.try_clone().unwrap());
    // count how many packets are sent?
    let mut counter: u64 = 0;

    loop {
        use nix::poll::PollFd;
        use nix::poll::PollFlags;
        use nix::poll::PollTimeout;
        use std::os::fd::AsFd;

        // to pool pipe read end
        let pipe_fd = PollFd::new(sigfile.as_fd(), PollFlags::POLLIN);
        // to pool tcp stream
        let tcp_fd = PollFd::new(stream.as_fd(), PollFlags::POLLIN);
        // to pool interface
        let if_fd = PollFd::new(iffile.as_fd(), PollFlags::POLLIN);
        // prepare input
        let mut fds = [pipe_fd, tcp_fd, if_fd];
        // https://docs.rs/nix/0.28.0/nix/poll/fn.poll.html
        let ret = nix::poll::poll(&mut fds, PollTimeout::NONE).unwrap();
        if ret <= 0 {
            panic!("Non positive nix::poll::poll");
        }
        let [pipe_fd, tcp_fd, if_fd] = fds;
        if pipe_fd.any().unwrap() {
            // consume pending signal data
            crate::signals::consume_sigpipe(sigfile);
            // send exit packet
            let exit_reason = 0_u32; // normal exit
            send_exit_pkt(&mut ostream, exit_reason);
            return false;
        }
        // check tcp connection
        let if_flag = if_fd.any().unwrap();
        let tcp_flag = tcp_fd.any().unwrap();
        // https://doc.rust-lang.org/std/mem/fn.drop.html
        // drop because if_fd borrowed fd used by iffile
        // std::mem::drop(if_fd);
        // std::mem::drop(tcp_fd);
        // drop is unnecessary because items are Copy
        // check interface
        if tcp_flag {
            if let Status::ExitOk = handle_remote2local_pkt(iffile, &mut istream, &mut buffer) {
                // remote endpoint exited
                println!("Remote exit!");
                return true;
            }
        }
        if if_flag {
            handle_local2remote_pkt(iffile, &mut ostream, &mut counter, &mut buffer);
        }
    }
}


use tokio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
// https://dtantsur.github.io/rust-openstack/tokio/io/struct.BufWriter.html
use std::process;
use std::os::fd::FromRawFd;
use std::os::fd::AsRawFd;
use std::io::Write;

const MAX_HEADER_SZ : usize = 64;
const DEF_BUF_SZ : usize = 4096;


// handle packet flow
// use 2 threads: [local->remote] and [remote->local]
pub async fn handle_flow(
    stream: &mut tokio::net::TcpStream,
    iffile: &mut tokio::fs::File,
    sigint: &mut tokio::signal::unix::Signal) -> () {

    // https://doc.rust-lang.org/std/os/fd/trait.FromRawFd.html
    // https://doc.rust-lang.org/nightly/std/os/fd/type.RawFd.html

    let iffd = iffile.as_raw_fd();
    let dupfd = unsafe { filedesc::FileDesc::duplicate_raw_fd(iffd).unwrap() };
    let mut out_iffile = unsafe { 
        std::fs::File::from_raw_fd(dupfd.as_raw_fd())
    };
    // let mut out_iffile = unsafe { 
    //     std::fs::File::from_raw_fd(
    //         filedesc::FileDesc::duplicate_raw_fd(
    //             iffile.as_raw_fd())
    //         .unwrap()
    //     )
    // };

    // buffer used to read from interface file
    let mut filebuf : [u8; DEF_BUF_SZ] = [0; DEF_BUF_SZ];
    // buffer used to read from TCP connection
    let mut tcpbuf : [u8; DEF_BUF_SZ] =  [0; DEF_BUF_SZ];

    let mut counter: u64 = 0;
    let mut stream_buf = tokio::io::BufWriter::with_capacity(MAX_HEADER_SZ + DEF_BUF_SZ, stream);
    'outer: loop {
        std::io::stdout().flush().unwrap();
        tokio::select! {
            // signal
            _ = sigint.recv() => {
                println!("SIGINT!");
                // build exit packet
                // pkt type = 2
                stream_buf.write(&(2 as u32).to_ne_bytes()).await.unwrap();
                // exit reason: 0 means normal shutdown
                stream_buf.write(&(0 as u32).to_ne_bytes()).await.unwrap();
                // send exit packet
                stream_buf.flush().await.unwrap();
                break;
            },
            // local to remote
            // https://docs.rs/tokio/latest/tokio/io/trait.AsyncReadExt.html#method.read
            n = iffile.read(&mut filebuf[..]) => {
                eprintln!("Read data from interface");
                if let Ok(sz) = n {
                    eprintln!("Read n = {} bytes", sz);
                    counter += 1;
                    // build exit packet
                    // pkt type = 2
                    stream_buf.write(&(1 as u32).to_ne_bytes()).await.unwrap();
                    // pkt length
                    stream_buf.write(&(sz as u32).to_ne_bytes()).await.unwrap();
                    // counter
                    stream_buf.write(&counter.to_ne_bytes()).await.unwrap();
                    // network packet
                    stream_buf.write(&filebuf[..sz]).await.unwrap();
                    // send packet
                    stream_buf.flush().await.unwrap();
                } else {
                    eprintln!("FATAL: reading from interface");
                    process::exit(1);
                }
            }
            // remote to local
            n = stream_buf.read_exact(&mut tcpbuf[..4]) => {
                if let Ok(4) = n {
                    let pkt_type: u32 = u32::from_ne_bytes(tcpbuf[..4].try_into().unwrap());
                    // parse packet
                    match pkt_type {
                        // data packet
                        1 => {
                            // read data length
                            let mut pkt_len: [u8; 4] = [0; 4];
                            stream_buf.read_exact(&mut pkt_len).await.unwrap();
                            let pkt_len: u32 = u32::from_ne_bytes(pkt_len);
                            // read counter
                            let mut counter: [u8; 8] = [0; 8];
                            stream_buf.read_exact(&mut counter).await.unwrap();
                            let _counter = u64::from_ne_bytes(counter);
                            // read data
                            stream_buf.read_exact(&mut tcpbuf[..(pkt_len as usize)]).await.unwrap();
                            // write to interface
                            eprintln!("out_iffile.write of {} bytes", pkt_len);
                            match out_iffile.write(&tcpbuf[0..(pkt_len as usize)]) {
                                Ok(_) => {
                                    eprintln!("\tOK!");
                                    ()
                                },
                                Err(e) => eprintln!("Error writing pkt to virtual interface: {}", e)
                            };
                        },
                        // exit packet
                        2 => {
                            // read exit reason
                            let mut exit_reason: [u8; 4] = [0; 4];
                            stream_buf.read_exact(&mut exit_reason).await.unwrap();
                            let exit_reason: u32 = u32::from_ne_bytes(exit_reason);
                            if exit_reason != 0 {
                                eprintln!("Protocol error: unknown exit_reason {} in VPN protocol", exit_reason);
                                break 'outer;
                            }
                        },
                        // error!
                        _ => {
                            eprintln!("Unknown pkt_type = {}", pkt_type);
                            process::exit(1);
                        }
                    }
                } else if let Ok(n) = n {
                    panic!("Nonsense {} length read!", n);
                } else {
                    eprintln!("SOCKET ERROR!");
                    break;
                }
            }
        }
    }
    // restore socket stream
    // stream = stream_buf.into_inner();
}

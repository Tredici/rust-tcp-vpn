
use crate::handshake;
//use crate::flows;
use crate::tunif;
use crate::async_flows;


use std::net::{IpAddr, TcpStream};
use std::process;
use tokio::{self,runtime::Builder};
use tokio::signal::unix::signal;
use tokio::signal::unix::SignalKind;


pub fn execute_client(ifname: String, ifaddr: IpAddr, netmask: u8, remote: std::net::SocketAddr) {
    // this part is synchronous
    let iffile = tunif::initialize_tun_interface(&ifname, ifaddr, netmask);
    // try to connect to remote server
    let mut stream = match TcpStream::connect(remote) {
        Ok(stream) => {
            println!("Connection established!");
            stream
        },
        Err(err) => {
            eprintln!("Cannot connect to: {} cause {}", remote, err);
            process::exit(1)
        }
    };
    // start handshake as client
    let h = handshake::handler_client_handshake(&mut stream, &ifaddr, netmask);
    if !h {
        eprintln!("Failed client handshake");
        std::process::exit(1)
    }
    // bring interface up
    tunif::set_interface_up(&iffile, &ifname);

    // fallback to async code
//    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();
    let fut = async move {
        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        stream.set_nonblocking(true).unwrap();
        let mut stream = tokio::net::TcpStream::from_std(stream).unwrap();
        let mut iffile = tokio::fs::File::from_std(iffile);
        async_flows::handle_flow(&mut stream, &mut iffile, &mut sigint).await;
    };
    rt.block_on(fut);
}

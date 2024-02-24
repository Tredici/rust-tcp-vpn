

pub mod client;
pub mod parsing;
pub mod handshake;
pub mod server;
pub mod async_flows;
pub mod flows;
pub mod tunif;

// How to use multiple module:
//  https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html


pub fn run(args: parsing::Args) {
    let ifname = args.interface.ifname;
    let ifaddr = args.interface.ifaddr;
    let netmask = args.interface.netmask;
    // different behaviour in case of client or server
    match args.mode {
        parsing::Mode::Client { remote } => client::execute_client(ifname, ifaddr, netmask, remote),
        parsing::Mode::Server { local } => server::execute_server(ifname, ifaddr, netmask, local),
    }
}



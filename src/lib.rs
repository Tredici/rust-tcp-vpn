pub mod client;
pub mod flows;
pub mod handshake;
pub mod parsing;
pub mod server;
pub mod signals;
pub mod tunif;

// How to use multiple module:
//  https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html

pub fn run(args: parsing::Args) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let ifname = args.interface.ifname;
    let ifaddr = args.interface.ifaddr;
    let netmask = args.interface.netmask;
    // different behaviour in case of client or server
    match args.mode {
        parsing::Mode::Client { remote } => client::execute_client(ifname, ifaddr, netmask, remote),
        parsing::Mode::Server { local } => server::execute_server(ifname, ifaddr, netmask, local),
    }
}

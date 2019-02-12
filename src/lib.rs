#[cfg(test)]
mod dummy;

use std::io;
use std::net::{TcpStream, SocketAddr};

pub struct LxiDevice {
    addr: SocketAddr,
    stream: TcpStream,
}

impl LxiDevice {
    pub fn connect(addr: SocketAddr) -> io::Result<LxiDevice> {
        let stream = TcpStream::connect(addr)?;
        Ok(LxiDevice {
            addr: stream.peer_addr().unwrap(),
            stream,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

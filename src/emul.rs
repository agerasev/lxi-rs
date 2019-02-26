use std::io::{prelude::*, self, BufReader, BufWriter};
use std::net::{SocketAddr, TcpListener};
use std::thread::{self, JoinHandle};

pub struct Emulator {
    listener: TcpListener,
}


impl Emulator {
    pub fn new(addr: (&str, u16)) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(Emulator {
            listener,
        })
    }

    pub fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn run(self) -> JoinHandle<io::Result<()>> {
        thread::spawn(move || {
            let stream = self.listener.incoming().next().unwrap()?;

            let mut reader = BufReader::new(stream.try_clone()?);
            let mut writer = BufWriter::new(stream);

            loop {
                let mut buf = Vec::new();

                match reader.read_until(b'\n', &mut buf)
                .map(|_| {
                    if buf.starts_with(b"*IDN?") {
                        &b"Emulator\r\n"[..]
                    } else if buf.starts_with(b"DATA?") {
                        &b"#14\0\xff\n\x80\r\n"[..]
                    } else {
                        &b"Error\r\n"[..]
                    }
                })
                .and_then(|response| writer.write_all(response))
                .and_then(|_| writer.flush()) {
                    Ok(_) => (),
                    Err(err) => match err.kind() {
                         io::ErrorKind::ConnectionAborted |
                         io::ErrorKind::BrokenPipe => break Ok(()),
                        _ => break Err(err),
                    },
                }
            }
        })
    }
}

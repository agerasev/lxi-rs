use std::io::prelude::*;
use std::io::{self, BufReader, BufWriter};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration};


pub struct LxiDevice {
    addr: (String, u16),
    stream: Option<LxiStream>,
    timeout: Option<Duration>,
}

struct LxiStream {
    inp: BufReader<TcpStream>,
    out: BufWriter<TcpStream>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LxiResponse {
    Text(String),
    Bin(Vec<u8>),
}


impl LxiDevice {
    pub fn new(addr: (String, u16), timeout: Option<Duration>) -> Self {
        Self { addr, stream: None, timeout }
    }

    pub fn address(&self) -> (&str, u16) {
        (self.addr.0.as_str(), self.addr.1)
    }

    pub fn set_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.timeout = timeout;
        match self.stream {
            Some(ref mut stream) => {
                stream.inp.get_mut().set_read_timeout(timeout)?;
                stream.out.get_mut().set_write_timeout(timeout)?;
            },
            None => (),
        }
        Ok(())
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    pub fn connect(&mut self) -> io::Result<()> {
        if self.is_connected() {
            return Err(io::ErrorKind::AlreadyExists.into())
        }
        let stream = match self.timeout {
            Some(to) => {
                self.address().to_socket_addrs().and_then(|mut addrs| {
                    addrs.next().ok_or(io::ErrorKind::NotFound.into())
                }).and_then(|addr| {
                    TcpStream::connect_timeout(&addr, to)
                })
            },
            None => TcpStream::connect(self.address()),
        }?;

        let inp = BufReader::new(stream.try_clone()?);
        let out = BufWriter::new(stream);
        let mut stream = LxiStream { inp, out };
        stream.set_timeout(self.timeout)?;
        self.stream = Some(stream);

        Ok(())
    }

    pub fn disconnect(&mut self) -> io::Result<()> {
        if !self.is_connected() {
            return Err(io::ErrorKind::NotConnected.into())
        }
        self.stream = None;
        Ok(())
    }

    pub fn reconnect(&mut self) -> io::Result<()> {
        self.disconnect()
        .and_then(|()| self.connect())
    }

    fn with_stream<R, F>(&mut self, mut f: F) -> io::Result<R>
    where F: FnMut(&mut LxiStream) -> io::Result<R> {
        self.stream.as_mut().ok_or(io::ErrorKind::NotConnected.into())
        .and_then(|stream| f(stream))
    }

    pub fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.with_stream(|stream| stream.send(data))
    }

    pub fn receive(&mut self) -> io::Result<LxiResponse> {
        self.with_stream(|stream| stream.receive())
    }

    pub fn send_timeout(&mut self, data: &[u8], timeout: Option<Duration>) -> io::Result<()> {
        self.with_stream(|stream| stream.send_timeout(data, timeout))
    }

    pub fn receive_timeout(&mut self, timeout: Option<Duration>) -> io::Result<LxiResponse> {
        self.with_stream(|stream| stream.receive_timeout(timeout))
    }
}

impl LxiStream {
    fn set_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.inp.get_mut().set_read_timeout(timeout)?;
        self.out.get_mut().set_write_timeout(timeout)?;
        Ok(())
    }

    fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.out.write_all(data)
        .and_then(|()| self.out.write_all(b"\r\n"))
        .and_then(|()| self.out.flush())
    }

    fn receive(&mut self) -> io::Result<LxiResponse> {
        let mut byte: [u8; 1] = [0];
        let mut buf = Vec::with_capacity(1);
        self.inp.read_exact(&mut byte)
        .and_then(|()| {
            if byte[0] != b'#' {
                // Ascii format
                self.inp.read_until(b'\n', &mut buf)
                .and_then(|_num| {
                    remove_newline(&mut buf);
                    Ok(buf)
                })
            } else {
                // Binary format
                self.inp.read_exact(&mut byte)
                .and_then(|()| {
                    (byte[0] as char).to_digit(10)
                })
            }
        })
    }

    fn send_timeout(&mut self, data: &[u8], to: Option<Duration>) -> io::Result<()> {
        let dto = self.out.get_ref().write_timeout()?;
        self.out.get_mut().set_write_timeout(to)?;
        let res = self.send(data);
        self.out.get_mut().set_write_timeout(dto)?;
        res
    }

    fn receive_timeout(&mut self, to: Option<Duration>) -> io::Result<LxiResponse> {
        let dto = self.out.get_ref().read_timeout()?;
        self.out.get_mut().set_read_timeout(to)?;
        let res = self.receive();
        self.out.get_mut().set_read_timeout(dto)?;
        res
    }
}

fn remove_newline(text: &mut Vec<u8>) {
    match text.pop() {
        Some(b'\n') => match text.pop() {
            Some(b'\r') => (),
            Some(c) => text.push(c),
            None => (),
        },
        Some(c) => text.push(c),
        None => (),
    }
}

impl LxiResponse {
    pub fn from_text<T: AsRef<str>>(t: &T) -> LxiResponse {
        LxiResponse::Text(String::from(t.as_ref()))
    }
    pub fn from_bin<T: AsRef<[u8]>>(t: &T) -> LxiResponse {
        LxiResponse::Bin(Vec::from(t.as_ref()))
    }
}

#[cfg(test)]
mod emul;

#[cfg(test)]
mod tests {
    use super::*;

    use std::thread;
    use std::time::{Duration};

    use emul::{Emulator};

    #[test]
    fn idn() {
        let e = Emulator::new(("localhost", 0)).unwrap();
        let p = e.address().unwrap().port();
        let e = e.run();

        thread::sleep(Duration::from_millis(100));

        {
            let mut d = LxiDevice::new((String::from("localhost"), p), None);
            d.connect().unwrap();
            d.send(b"*IDN?").unwrap();
            assert_eq!(d.receive().unwrap(), LxiResponse::from_text(&"Emulator"));
        }

        e.join().unwrap().unwrap();
    }
}

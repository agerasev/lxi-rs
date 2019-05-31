use std::io::prelude::*;
use std::io::{self, BufReader, BufWriter};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration};
use std::marker::PhantomData;


/// Hook for LXI response parsing
pub trait LxiHook {
    type Output;
    fn read(stream: &mut BufReader<TcpStream>) -> io::Result<Self::Output>;
}

/// Defaut hook that reads only text ending with a newline
pub struct LxiTextHook {}

impl LxiHook for LxiTextHook {
    type Output = Vec<u8>;
    fn read(stream: &mut BufReader<TcpStream>) -> io::Result<Self::Output> {
        let mut buf = Vec::new();
        stream.read_until(b'\n', &mut buf)
        .and_then(|_num| {
            remove_newline(&mut buf);
            Ok(buf)
        })
    }
}

/// Abstract LXI device we can connect and read/write data
pub struct LxiDevice<H: LxiHook = LxiTextHook> {
    addr: (String, u16),
    stream: Option<LxiStream>,
    timeout: Option<Duration>,
    _ph: PhantomData<H>,
}

pub type LxiTextDevice = LxiDevice<LxiTextHook>;

struct LxiStream {
    inp: BufReader<TcpStream>,
    out: BufWriter<TcpStream>,
}

impl<H: LxiHook> LxiDevice<H> {
    pub fn new(addr: (String, u16), timeout: Option<Duration>) -> Self {
        Self { addr, stream: None, timeout, _ph: PhantomData }
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

    pub fn receive(&mut self) -> io::Result<H::Output> {
        self.with_stream(|stream| stream.receive::<H>())
    }

    pub fn send_timeout(&mut self, data: &[u8], timeout: Option<Duration>) -> io::Result<()> {
        self.with_stream(|stream| stream.send_timeout(data, timeout))
    }

    pub fn receive_timeout(&mut self, timeout: Option<Duration>) -> io::Result<H::Output> {
        self.with_stream(|stream| stream.receive_timeout::<H>(timeout))
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

    fn receive<H: LxiHook>(&mut self) -> io::Result<H::Output> {
        H::read(&mut self.inp)
    }

    fn send_timeout(&mut self, data: &[u8], to: Option<Duration>) -> io::Result<()> {
        let dto = self.out.get_ref().write_timeout()?;
        self.out.get_mut().set_write_timeout(to)?;
        let res = self.send(data);
        self.out.get_mut().set_write_timeout(dto)?;
        res
    }

    fn receive_timeout<H: LxiHook>(&mut self, to: Option<Duration>) -> io::Result<H::Output> {
        let dto = self.inp.get_ref().read_timeout()?;
        self.inp.get_mut().set_read_timeout(to)?;
        let res = self.receive::<H>();
        self.inp.get_mut().set_read_timeout(dto)?;
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


#[cfg(test)]
mod emul;

#[cfg(test)]
mod tests {
    use super::*;

    use std::thread;
    use std::time::{Duration};

    use emul::{Emulator};

    #[test]
    fn emulate() {
        let e = Emulator::new(("localhost", 0)).unwrap();
        let p = e.address().unwrap().port();
        let e = e.run();

        thread::sleep(Duration::from_millis(100));

        {
            let mut d = LxiTextDevice::new((String::from("localhost"), p), None);
            d.connect().unwrap();

            d.send(b"*IDN?").unwrap();
            assert_eq!(d.receive().unwrap(), Vec::from("Emulator"));
        }

        e.join().unwrap().unwrap();
    }
}

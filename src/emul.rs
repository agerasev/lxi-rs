use std::io::{prelude::*, self, BufReader, BufWriter};
use std::net::{SocketAddr, TcpListener, TcpStream, Shutdown};
use std::thread::{self, JoinHandle};
use std::sync::{Arc};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Emulator {
    listener: TcpListener,
    clients: Vec<(TcpStream, JoinHandle<()>)>,
    exit: Arc<AtomicBool>,
}

pub struct EmulatorHandle {
    address: SocketAddr,
    thread: Option<JoinHandle<()>>,
    exit: Arc<AtomicBool>,
}

impl Emulator {
    pub fn new(addr: (&str, u16)) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(Emulator {
            listener,
            clients: Vec::new(),
            exit: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn handle_client(&mut self, stream: TcpStream) -> io::Result<()> {
        self.clients.push((
            stream.try_clone()?,
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut writer = BufWriter::new(stream);

                loop {
                    let mut buf = Vec::new();

                    match reader.read_until(b'\n', &mut buf)
                    .map(|_| {
                        if buf.starts_with(b"*IDN?") {
                            &b"Emulator\r\n"[..]
                        } else {
                            &b"Error\r\n"[..]
                        }
                    })
                    .and_then(|response| writer.write_all(response))
                    .and_then(|_| writer.flush()) {
                        Ok(_) => (),
                        Err(err) => match err.kind() {
                            io::ErrorKind::BrokenPipe => break,
                            _ => panic!("{:?}", err),
                        },
                    };
                }
            }),
        ));

        Ok(())
    }

    pub fn run(mut self) -> io::Result<EmulatorHandle> {
        let address = self.address()?;
        let exit = self.exit.clone();

        let thread = thread::spawn(move || {
            let listener = self.listener.try_clone().unwrap();
            for stream in listener.incoming() {
                if self.exit.load(Ordering::SeqCst) {
                    break;
                }
                self.handle_client(stream.unwrap()).unwrap();
            }
        });

        Ok(EmulatorHandle { address, thread: Some(thread), exit })
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        while let Some((stream, thread)) = self.clients.pop() {
            stream.shutdown(Shutdown::Both).unwrap();
            thread.join().unwrap();
        }
    }
}

impl EmulatorHandle {
    pub fn shutdown(&mut self) -> io::Result<()> {
        if self.thread.is_none() {
            return Ok(())
        }
        self.exit.store(true, Ordering::SeqCst);
        TcpStream::connect(self.address)?;
        match self.thread.take().unwrap().join() {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        }
        Ok(())
    }
}

impl Drop for EmulatorHandle {
    fn drop(&mut self) {
        self.shutdown().unwrap();
    }
}
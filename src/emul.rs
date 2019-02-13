use std::io::{prelude::*, self, BufReader, BufWriter};
use std::net::{SocketAddr, TcpListener, TcpStream, Shutdown};
use std::thread::{self, JoinHandle};
use std::sync::{Arc, Mutex};
use std::collections::{HashMap};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Emulator {
    listener: TcpListener,
    streams: Arc<Mutex<HashMap<usize, Option<(TcpStream, JoinHandle<()>)>>>>,
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
            streams: Arc::new(Mutex::new(HashMap::new())),
            exit: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn handle_client(&self, stream: TcpStream, id: usize) -> io::Result<()> {
        let streams = self.streams.clone();
        let mut guard = self.streams.lock().unwrap();

        assert!(guard.insert(id, Some((
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

                {
                    let mut guard = streams.lock().unwrap();
                    guard.remove(&id).unwrap();
                }
            }),
        ))).is_none());

        Ok(())
    }

    pub fn run(self) -> io::Result<EmulatorHandle> {
        let address = self.address()?;
        let exit = self.exit.clone();

        let thread = thread::spawn(move || {
            let mut id = 1;
            for stream in self.listener.incoming() {
                if self.exit.load(Ordering::SeqCst) {
                    break;
                }
                self.handle_client(stream.unwrap(), id).unwrap();
                id += 1;
            }
        });

        Ok(EmulatorHandle { address, thread: Some(thread), exit })
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        let mut guard = self.streams.lock().unwrap();
        for (_id, opt) in guard.iter_mut() {
            let (stream, thread) = opt.take().unwrap();
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
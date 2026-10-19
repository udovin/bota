//! A live connection to a server: the hub, or the game server of a match.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use std::time::Duration;

use bota_proto::{FrameReader, Message, encode_frame_to_vec};

/// How long one address is tried before the next, or before giving up.
const CONNECT_WAIT: Duration = Duration::from_secs(3);

/// A socket with a reader thread behind it, the server saying `M`.
///
/// Messages pile up in a channel; [`Net::poll`] hands over whatever has
/// arrived without ever blocking the render loop. Dropping it hangs up.
pub struct Net<M> {
    stream: TcpStream,
    inbox: Receiver<M>,
    closed: bool,
}

impl<M: Message> Net<M> {
    /// Connects to the first address the name gives that answers in
    /// [`CONNECT_WAIT`], and starts reading.
    pub fn connect(addr: &str) -> std::io::Result<Net<M>> {
        let mut failed = None;
        for at in addr.to_socket_addrs()? {
            match TcpStream::connect_timeout(&at, CONNECT_WAIT) {
                Ok(stream) => return Net::over(stream),
                Err(err) => failed = Some(err),
            }
        }
        Err(failed.unwrap_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "no address to connect to")
        }))
    }

    /// Starts reading a connected socket.
    fn over(stream: TcpStream) -> std::io::Result<Net<M>> {
        stream.set_nodelay(true)?;
        let reader_stream = stream.try_clone()?;
        let (tx, inbox) = channel();
        thread::spawn(move || {
            let mut reader = FrameReader::new();
            let mut stream = reader_stream;
            let mut buf = [0u8; 65536];
            loop {
                let n = match stream.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                reader.push(&buf[..n]);
                loop {
                    match reader.next_message::<M>() {
                        Ok(Some(msg)) => {
                            if tx.send(msg).is_err() {
                                return;
                            }
                        }
                        Ok(None) => break,
                        Err(_) => return,
                    }
                }
            }
        });
        Ok(Net {
            stream,
            inbox,
            closed: false,
        })
    }

    /// Everything that arrived since the last poll.
    pub fn poll(&mut self) -> Vec<M> {
        let mut heard = Vec::new();
        loop {
            match self.inbox.try_recv() {
                Ok(msg) => heard.push(msg),
                Err(TryRecvError::Empty) => return heard,
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    return heard;
                }
            }
        }
    }

    /// Sends one message. A failed send hangs up.
    pub fn send<T: Message>(&mut self, msg: &T) {
        let Ok(frame) = encode_frame_to_vec(msg) else {
            return;
        };
        if self.stream.write_all(&frame).is_err() {
            let _ = self.stream.shutdown(Shutdown::Both);
        }
    }

    /// Whether the server is gone and everything it said has been polled.
    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

impl<M> Drop for Net<M> {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

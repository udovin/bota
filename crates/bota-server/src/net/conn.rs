//! One connected peer: reader thread, writer thread, outbox.

use std::io::Read;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;

use bota_proto::{FrameReader, Message, PlayerId, encode_frame_to_vec};

use crate::net::Outbox;

/// Everything that reaches a server thread from the network, `M` being what
/// its peers say.
pub enum NetEvent<M> {
    /// A peer finished the TCP handshake.
    Connected(TcpStream),
    /// A peer sent a message.
    Msg(PlayerId, M),
    /// A peer's connection ended, cleanly or not.
    Disconnected(PlayerId),
}

/// The server-side handle of one peer.
pub struct Connection {
    /// Wire identity of the peer.
    pub id: PlayerId,
    /// Frames on their way out.
    pub outbox: Arc<Outbox>,
    /// The thread putting them on the socket, kept so the last of them can be
    /// waited for.
    writer: Option<thread::JoinHandle<()>>,
}

impl Connection {
    /// Adopts a stream: spawns its reader and writer threads.
    ///
    /// What the peer says is read as `M`, and reaches the receiving thread
    /// wrapped into whatever it listens for.
    pub fn spawn<M, E>(id: PlayerId, stream: TcpStream, events: Sender<E>) -> Connection
    where
        M: Message,
        E: From<NetEvent<M>> + Send + 'static,
    {
        let _ = stream.set_nodelay(true);
        let outbox = Outbox::new();
        let writer_outbox = Arc::clone(&outbox);
        let writer_stream = stream.try_clone().expect("cloning a tcp stream");
        let writer = thread::spawn(move || writer_outbox.run_writer(writer_stream));
        thread::spawn(move || read_loop::<M, E>(id, stream, events));
        Connection {
            id,
            outbox,
            writer: Some(writer),
        }
    }

    /// Queues a message that must arrive.
    pub fn send<T: Message>(&self, msg: &T) {
        if let Ok(frame) = encode_frame_to_vec(msg) {
            self.outbox.push(frame);
        }
    }

    /// Queues a snapshot frame, replacing any still-waiting one.
    pub fn send_snapshot(&self, frame: Vec<u8>) {
        self.outbox.push_snapshot(frame);
    }

    /// Whether the peer is still worth sending to.
    pub fn is_open(&self) -> bool {
        !self.outbox.is_closed()
    }

    /// Stops sending and waits for what is queued to reach the socket.
    ///
    /// Waited for rather than left to finish on its own: the match's last
    /// message is queued at the moment the server has nothing else to do, and
    /// a process that exits from under the writer takes that message with it.
    pub fn close_and_wait(&mut self) {
        self.close();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }

    /// Stops sending and lets the writer drain and hang up.
    pub fn close(&self) {
        self.outbox.close();
    }
}

fn read_loop<M, E>(id: PlayerId, mut stream: TcpStream, events: Sender<E>)
where
    M: Message,
    E: From<NetEvent<M>>,
{
    let mut reader = FrameReader::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        reader.push(&buf[..n]);
        loop {
            match reader.next_message::<M>() {
                Ok(Some(msg)) => {
                    if events.send(NetEvent::Msg(id, msg).into()).is_err() {
                        return;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    let _ = events.send(NetEvent::<M>::Disconnected(id).into());
                    return;
                }
            }
        }
    }
    let _ = events.send(NetEvent::<M>::Disconnected(id).into());
}

/// The accept thread body: hands every incoming stream to the server thread,
/// as a peer that will say `M`.
pub fn accept_loop<M, E>(listener: std::net::TcpListener, events: Sender<E>)
where
    E: From<NetEvent<M>>,
{
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if events
                    .send(NetEvent::<M>::Connected(stream).into())
                    .is_err()
                {
                    return;
                }
            }
            Err(_) => continue,
        }
    }
}

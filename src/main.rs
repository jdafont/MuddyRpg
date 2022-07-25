use std::collections::HashMap;
use std::iter::Map;

use bytes::*;
use tokio::net::tcp::{OwnedWriteHalf, OwnedReadHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::sync::mpsc::*;

mod serialize;

fn pop(barry: &[u8]) -> [u8; 4] {
    barry.try_into().expect("slice with incorrect length")
}
type Error = Box<dyn std::error::Error>;
type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
struct InitRequestData {
    username: String
}

#[derive(Debug)]
struct InitResponseData {
    object_id: u32
}

#[derive(Debug)]
enum Packet {
    InitRequest(InitRequestData),
    InitResponse(InitResponseData)
}

impl Packet {
    async fn send_packet(self, tx: &mut OwnedWriteHalf) -> std::result::Result<(), std::io::Error>{
        let mut buf = BytesMut::new();
        match self {
            Packet::InitResponse(data) => {
                buf.put_u32_le(1);
                buf.put_u32_le(data.object_id);
            },
            _ => {}
        }

        return tx.write_all_buf(&mut buf).await;
    }
}

#[derive(Debug)]
struct PacketFrame {
    connection_id: u32,
    packet: Packet
}

#[derive(Debug)]
struct Connection {
    id: u32,
    rx: Receiver<PacketFrame>,
    tx: OwnedWriteHalf
}

async fn handle_client(mut socket: OwnedReadHalf, id: u32, tx: Sender<PacketFrame>) -> Result<()>{
    // In a loop, read data from the socket and write the data back.
    loop {
        let mut buf = BytesMut::with_capacity(1024);

        let read_result = socket.read_buf(&mut buf).await;
        let mut buf = buf.freeze();

        let packet = match read_result {
            // socket closed
            Ok(n) if n == 0 => {
                return Ok(());
            },
            Ok(n) => {

                let username = serialize::read_string(&mut buf);

                Packet::InitRequest(InitRequestData {
                    username
                })
            },
            Err(e) => {
                eprintln!("failed to read from socket; err = {:?}", e);
                return Ok(());
            }
        };

        let pf = PacketFrame {
            packet,
            connection_id: id
        };

        tx.send(pf).await?;
    }
}

struct Player {
    username: String,
    object_id: u32
}

struct GameState {
    connections: HashMap<u32, Connection>
}

async fn run_server(mut connection_rx: Receiver<Connection>) {
    let mut object_id = 0;

    let mut state = GameState {
        connections: HashMap::new()
    };

    loop {
        // Check for connections
        loop {
            match connection_rx.try_recv() {
                Ok(conn) => {
                    println!("Just connected {}", &conn.id);
                    let _ = state.connections.insert(conn.id, conn);
                },
                Err(error::TryRecvError::Empty) => {
                    break
                },
                Err(x) => {
                    eprintln!("Error receiving connections: {}", x);
                    break
                }

            }    
        }

        let mut packets = vec![];
        let mut drop_conns = vec![];

        // Check for packets
        for (id, conn) in state.connections.iter_mut() {
            // Handle packets
            loop {
                match conn.rx.try_recv() {
                    Ok(packet_frame) => {
                        println!("Got a packet from {}", &conn.id);
                        packets.push(packet_frame);
                    },
                    Err(error::TryRecvError::Empty) => {
                        break
                    },
                    Err(x) => {
                        drop_conns.push(conn.id);
                        eprintln!("Connection dropped from {}", id);
                        break
                    }
                }
            }
        }
        for id in drop_conns {
            state.connections.remove(&id).unwrap();
        }

        for packet_frame in packets {
            match packet_frame.packet {
                Packet::InitRequest(data) => {
                    let name = data.username;
                    object_id += 1;
                    let id = object_id;

                    let response = Packet::InitResponse(InitResponseData {
                        object_id: id
                    });

                    let conn = state.connections.get_mut(&packet_frame.connection_id).unwrap();

                    let resp = response.send_packet(&mut conn.tx).await;
                    resp.expect("Failed to write packet");
                    println!("Wrote a packet to {}", &conn.id);
                },
                _ => {
                    eprintln!("Unexpected packet recv'd");
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let (conn_tx, conn_rx) = mpsc::channel::<Connection>(32);

    tokio::spawn(async move {
        run_server(conn_rx).await;
    });

    let addr = "127.0.0.1:8083";
    let listener = TcpListener::bind(addr).await?;
    println!("Bound to {addr}");

    let mut i = 0;
    loop {
        let (socket, _sock_addr) = listener.accept().await?;
        let (sock_read, sock_write) = socket.into_split();
        i += 1;

        let (tx, rx) = mpsc::channel::<PacketFrame>(32);

        let connection = Connection {
            id: i,
            rx,
            tx: sock_write
        };

        let _ = conn_tx.send(connection).await?;

        println!("Accepted a socket conn");
        tokio::spawn(async move {
            let _result = handle_client(sock_read, i, tx).await;
        });
    }
}
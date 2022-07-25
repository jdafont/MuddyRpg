use std::collections::HashMap;

use bytes::*;
use tokio::net::tcp::{OwnedWriteHalf, OwnedReadHalf};
use tokio::net::{TcpListener};
use tokio::io::{AsyncReadExt};
use tokio::sync::mpsc;
use tokio::sync::mpsc::*;

mod serialize;
mod packets;

use crate::packets::*;

type Error = Box<dyn std::error::Error>;
type Result<T> = core::result::Result<T, Error>;

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
        let buf = buf.freeze();

        let packet = match read_result {
            // socket closed
            Ok(n) if n == 0 => {
                return Ok(());
            },
            Ok(_n) => {
                Packet::read_packet(buf)
            },
            Err(e) => {
                eprintln!("failed to read from socket; err = {:?}", e);
                return Ok(());
            }
        };

        match packet {
            Some(p) => {

                let pf = PacketFrame {
                    packet: p,
                    connection_id: id
                };

                tx.send(pf).await?;
            },
            None => {
                eprintln!("Failed to parse a packet");
            }
        }

    }
}

struct Player {
    username: String,
    connection_id: u32,
    object_id: u32
}

struct GameState {
    connections: HashMap<u32, Connection>,
    players: Vec<Player>
}

async fn run_server(mut connection_rx: Receiver<Connection>) {
    let mut object_id = 0;

    let mut state = GameState {
        connections: HashMap::new(),
        players: vec![]
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
                    Err(_x) => {
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
                Packet::InitUserRequest(data) => {
                    let name = data.username;
                    object_id += 1;
                    let id = object_id;

                    let response = Packet::InitUserResponse(InitUserResponseData {
                        object_id: id
                    });

                    let conn = state.connections.get_mut(&packet_frame.connection_id).unwrap();

                    let resp = response.send_packet(&mut conn.tx).await;
                    resp.expect("Failed to write packet");
                    println!("Wrote a packet to {}", &conn.id);

                    state.players.push(Player {
                        connection_id: conn.id,
                        object_id: id,
                        username: name
                    });
                },
                Packet::MoveRequest(data) => {

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
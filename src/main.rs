use std::collections::HashMap;
use std::collections::hash_map::ValuesMut;

use bytes::*;
use tokio::net::tcp::{OwnedWriteHalf, OwnedReadHalf};
use tokio::net::{TcpListener};
use tokio::io::{AsyncReadExt};
use tokio::sync::mpsc;
use tokio::sync::mpsc::*;

mod serialize;
mod packets;

use crate::packets::*;

extern crate nalgebra as na;

type Error = Box<dyn std::error::Error>;
type Result<T> = core::result::Result<T, Error>;

const DELTA: f32 = 1.0/60.0;
const SPEED: f32 = 200.0;

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

struct Position {
    x: f32,
    y: f32
}

struct Player {
    username: String,
    connection_id: u32,
    object_id: u32,
    position: Position
}

struct GameState {
    connections: HashMap<u32, Connection>,
    players: HashMap<u32, Player>
}

async fn broadcast_packet(connections: &mut HashMap<u32, Connection>, packet: Packet, ignore_id: u32) -> Result<()> {
    for conn in connections.values_mut() {
        if conn.id == ignore_id {
            continue;
        }

        packet.send_packet(&mut conn.tx).await?;
    }
    return Ok(())
}

async fn run_server(mut connection_rx: Receiver<Connection>) -> ! {
    let mut object_id = 0;

    let mut state = GameState {
        connections: HashMap::new(),
        players: HashMap::new()
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

        for id in drop_conns.iter() {
            state.connections.remove(&id).unwrap();
        }

        for id in drop_conns.iter() {
            if let Some(player) = state.players.get(id) {
                let remove_player = Packet::ObjectRemovedEvent(ObjectRemovedEventData {
                    object_id: player.object_id
                });
                let _ = broadcast_packet(&mut state.connections, remove_player, *id).await;
            }
            let _ = state.players.remove(id);
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

                    for (_connection_id, player) in state.players.iter() {
                        let new_player = Packet::NewObjectEvent(NewObjectEventData {
                            name: player.username.clone(),
                            object_id: player.object_id,
                            x: player.position.x,
                            y: player.position.y
                        });
                        let _ = new_player.send_packet(&mut conn.tx).await;
                    }

                    state.players.insert(conn.id, Player {
                        connection_id: conn.id,
                        object_id: id,
                        username: name.clone(),
                        position: Position {
                            x: 0.0,
                            y: 0.0
                        }
                    });

                    let new_player = Packet::NewObjectEvent(NewObjectEventData {
                        name: name.clone(),
                        object_id: id,
                        x: 0.0,
                        y: 0.0
                    });
                    let _ = broadcast_packet(&mut state.connections, new_player, packet_frame.connection_id).await;
                },
                Packet::MoveRequest(data) => {
                    let a_x: f32 = {
                        if data.right { 1.0 }
                        else if data.left { -1.0 }
                        else { 0.0 }
                    };

                    let a_y: f32 = {
                        if data.down { 1.0 }
                        else if data.up { -1.0 }
                        else { 0.0 }
                    };

                    let conn = state.connections.get_mut(&packet_frame.connection_id).unwrap();

                    let player = state.players.get_mut(&packet_frame.connection_id).expect("Could not find player by conn id");

                    let stored_pos = &mut player.position;

                    if a_x == 0.0 && a_y == 0.0 {
                        let resp = Packet::MoveResponse(MoveResponseData {
                            confirm: true,
                            new_x: stored_pos.x,
                            new_y: stored_pos.y
                        });

                        let _ = resp.send_packet(&mut conn.tx).await;
                        continue;
                    }
                    let pos = na::Vector2::new(stored_pos.x, stored_pos.y);
                    let accel = na::Vector2::new(a_x, a_y).normalize();
                    let new_pos = pos + SPEED * DELTA * accel;
                    let resp = Packet::MoveResponse(MoveResponseData {
                        confirm: true,
                        new_x: new_pos.x,
                        new_y: new_pos.y
                    });

                    stored_pos.x = new_pos.x;
                    stored_pos.y = new_pos.y;

                    let _ = resp.send_packet(&mut conn.tx).await;

                    let player_moved = Packet::ObjectMovedEvent(ObjectMovedEventData {
                        object_id: player.object_id,
                        x: new_pos.x,
                        y: new_pos.y
                    });

                    let _ = resp.send_packet(&mut conn.tx).await;
                    broadcast_packet(&mut state.connections, player_moved, packet_frame.connection_id).await;
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
        socket.set_nodelay(true)?;
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